use super::*;
pub(super) const MAX_POINTS: usize = 8;
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
                let friction = (ca.friction * cb.friction).sqrt();
                let restitution = (ca.restitution + cb.restitution) * 0.5;
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
                            tangent_impulse =
                                tangent_impulse.clamp_length_max(friction * normal_impulse);
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
                        tangent_impulse,
                        bounce: if speed < -1. {
                            -speed * restitution
                        } else {
                            0.
                        },
                        friction,
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
