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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactEvent {
    Started(ColliderHandle, ColliderHandle),
    Stopped(ColliderHandle, ColliderHandle),
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
    tangent_impulse: Vec3,
    bounce: f32,
    friction: f32,
    key: (ColliderHandle, ColliderHandle),
    feature: u64,
    cache: Option<(usize, usize)>,
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
    joint_solver: Option<fn(&mut Self, JointPhase, f32, &mut StepStats)>,
    disabled_joint_pairs: Vec<(RigidBodyHandle, RigidBodyHandle)>,
    planar: bool,
    contacts: Vec<ContactPair>,
    next_contacts: Vec<ContactPair>,
    manifolds: Vec<Manifold>,
    next_manifolds: Vec<Manifold>,
    next_feature: u64,
    settings: SolverSettings,
    islands: Islands,
    last_gravity: Vec3,
    old_positions: Vec<(RigidBodyHandle, Vec3, Quat)>,
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
            joint_solver: None,
            disabled_joint_pairs: Vec::new(),
            planar,
            contacts: Vec::new(),
            next_contacts: Vec::new(),
            manifolds: Vec::new(),
            next_manifolds: Vec::new(),
            next_feature: 1,
            settings: Default::default(),
            islands: Default::default(),
            last_gravity: gravity,
            old_positions: Vec::new(),
            mass_updates: Vec::new(),
            events: Vec::new(),
            constraints: Vec::new(),
            proxies: Vec::new(),
            points: Vec::with_capacity(8),
            last_step: StepStats::default(),
        }
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
    pub fn contact_pair(&self, a: ColliderHandle, b: ColliderHandle) -> Option<&ContactPair> {
        let key = pair_key(a, b);
        self.contacts
            .binary_search_by_key(&key, |c| pair_key(c.collider1, c.collider2))
            .ok()
            .map(|i| &self.contacts[i])
    }
    pub fn contact_events(&self) -> &[ContactEvent] {
        &self.events
    }
    pub fn contacts(&self) -> impl Iterator<Item = &ContactPair> {
        self.contacts.iter()
    }
    /// Advance a fixed simulation interval in (0, 0.1] seconds. Forces apply for
    /// this complete interval and are then cleared. The caller owns accumulation.
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
        for _ in 0..substeps {
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
            stats.ccd_impacts += self.sweep_balls();
            self.find_contacts(h, &mut stats);
            self.wake_contact_islands();
            self.warm_start(&mut stats);
            self.run_joints(JointPhase::Prepare, h, &mut stats);
            self.solve_velocities(&mut stats);
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
    fn sweep_balls(&mut self) -> usize {
        let mut hits = 0;
        for &(id, start_origin, start_rotation) in &self.old_positions {
            let body = self.bodies[id].core();
            if !body.ccd || body.kind != Kind::Dynamic {
                continue;
            }
            let mut closest = 1.;
            let mut correction = None;
            for &child in &body.colliders {
                let collider = &self.colliders[child];
                if collider.sensor {
                    continue;
                }
                let Shape::Ball(radius) = collider.shape else {
                    continue;
                };
                let start = start_origin + start_rotation * collider.local_translation;
                let end = collider.pose(body).position;
                for &(other, old_origin, old_rotation) in &self.old_positions {
                    if other == id || self.joint_contacts_disabled(id, other) {
                        continue;
                    }
                    let target = self.bodies[other].core();
                    for &target_child in &target.colliders {
                        let tc = &self.colliders[target_child];
                        if tc.sensor
                            || collider.membership & tc.filter == 0
                            || tc.membership & collider.filter == 0
                        {
                            continue;
                        }
                        let target_pose = tc.pose(target);
                        let target_start = old_origin + old_rotation * tc.local_translation;
                        let relative_start = start - target_start + target_pose.position;
                        if relative_start.distance_squared(end) < radius * radius * 0.04 {
                            continue;
                        }
                        let swept = Bounds {
                            min: relative_start.min(end) - Vec3::splat(radius),
                            max: relative_start.max(end) + Vec3::splat(radius),
                        };
                        if !swept.intersects(geometry::bounds(
                            tc.shape,
                            target_pose.position,
                            target_pose.rotation,
                        )) {
                            continue;
                        }
                        if let Some((t, n)) = geometry::sweep_ball(
                            relative_start,
                            end,
                            radius,
                            tc.shape,
                            target_pose.position,
                            target_pose.rotation,
                            self.planar,
                        ) && t < closest
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
                hits += 1;
            }
        }
        hits
    }
    fn solve_velocities(&mut self, stats: &mut StepStats) {
        for _ in 0..self.settings.velocity_iterations {
            for c in &mut self.constraints {
                let (a, b) = self.bodies.two_mut(c.a, c.b);
                let a = a.core_mut();
                let b = b.core_mut();
                if a.sleeping && b.sleeping
                    || a.sleeping && b.kind != Kind::Dynamic
                    || b.sleeping && a.kind != Kind::Dynamic
                {
                    continue;
                }
                stats.velocity_constraints += 1;
                let ra = c.point - a.world_com();
                let rb = c.point - b.world_com();
                let denom = effective_mass(a, b, ra, rb, c.normal);
                if denom <= 1e-10 {
                    continue;
                }
                let relative = b.point_velocity(c.point) - a.point_velocity(c.point);
                let change = (c.bounce - relative.dot(c.normal)) / denom;
                let accumulated = (c.normal_impulse + change).max(0.);
                let j = c.normal * (accumulated - c.normal_impulse);
                c.normal_impulse = accumulated;
                a.impulse(-j, c.point);
                b.impulse(j, c.point);
                let relative = b.point_velocity(c.point) - a.point_velocity(c.point);
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
                let new = candidate.clamp_length_max(c.friction * c.normal_impulse);
                let j = new - c.tangent_impulse;
                c.tangent_impulse = new;
                a.impulse(-j, c.point);
                b.impulse(j, c.point);
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
