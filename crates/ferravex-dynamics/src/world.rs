use crate::{collision::*, math::*, *};

struct Slot<const D: usize> {
    generation: u32,
    body: Option<Body<D>>,
}
#[derive(Clone, Copy)]
struct Proxy<const D: usize> {
    index: usize,
    min: [f32; D],
    max: [f32; D],
}
#[derive(Clone, Copy)]
struct Motion<const D: usize> {
    index: usize,
    position: [f32; D],
    velocity: [f32; D],
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepStats {
    pub bodies: usize,
    pub broadphase_tests: usize,
    pub candidate_pairs: usize,
    pub contacts: usize,
    pub ccd_impacts: usize,
    /// Remaining movement was discarded to stay conservative after this body's impact budget.
    pub ccd_budget_stops: usize,
}

/// Reuses bounded scratch buffers and returns an error rather than dropping
/// overflowing collision pairs. Failed steps restore all body motion and forces.
/// Buffers grow lazily to the configured bounds, then reuse their storage.
pub struct PhysicsWorld<const D: usize> {
    config: WorldConfig<D>,
    slots: Vec<Slot<D>>,
    free: Vec<usize>,
    len: usize,
    proxies: Vec<Proxy<D>>,
    pairs: Vec<(usize, usize)>,
    backup: Vec<Motion<D>>,
    contacts: Vec<Contact<D>>,
    next_contacts: Vec<Contact<D>>,
    events: Vec<ContactEvent<D>>,
    pending_stops: Vec<ContactEvent<D>>,
}
impl<const D: usize> PhysicsWorld<D> {
    /// Construct a world with default finite work/memory limits.
    /// Panics for dimensions other than 2/3 or invalid gravity; use `with_config`
    /// for fallible construction from untrusted settings.
    pub fn new(gravity: [f32; D]) -> Self {
        Self::with_config(WorldConfig::new(gravity)).expect("valid physics world configuration")
    }
    pub fn with_config(config: WorldConfig<D>) -> Result<Self, PhysicsError> {
        if D != 2 && D != 3 {
            return Err(PhysicsError::InvalidDimension);
        }
        if !valid(config.gravity)
            || config.max_bodies == 0
            || config.max_bodies > 1_000_000
            || config.max_pairs == 0
            || config.max_pairs > 10_000_000
            || config.max_pair_tests == 0
            || config.max_pair_tests > 100_000_000
            || !(1..=64).contains(&config.solver_iterations)
            || !(1..=16).contains(&config.ccd_iterations)
            || !config.contact_slop.is_finite()
            || !(1e-6..=0.1).contains(&config.contact_slop)
        {
            return Err(PhysicsError::InvalidConfiguration);
        }
        Ok(Self {
            config,
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
            proxies: Vec::new(),
            pairs: Vec::new(),
            backup: Vec::new(),
            contacts: Vec::new(),
            next_contacts: Vec::new(),
            events: Vec::new(),
            pending_stops: Vec::new(),
        })
    }
    pub fn config(&self) -> WorldConfig<D> {
        self.config
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn set_gravity(&mut self, gravity: [f32; D]) -> Result<(), PhysicsError> {
        if !valid(gravity) {
            return Err(PhysicsError::InvalidConfiguration);
        }
        self.config.gravity = gravity;
        Ok(())
    }
    pub fn insert(&mut self, desc: BodyDesc<D>) -> Result<BodyHandle, PhysicsError> {
        if !desc.valid() || desc.kind == BodyKind::Static && desc.velocity != [0.; D] {
            return Err(PhysicsError::InvalidBody);
        }
        if self.len >= self.config.max_bodies {
            return Err(PhysicsError::BodyCapacity);
        }
        let body = Body {
            desc,
            force: [0.; D],
        };
        let index = if let Some(index) = self.free.pop() {
            self.slots[index].body = Some(body);
            index
        } else {
            // Retired generation-overflow slots count against storage capacity.
            if self.slots.len() >= self.config.max_bodies {
                return Err(PhysicsError::BodyCapacity);
            }
            self.slots.push(Slot {
                generation: 0,
                body: Some(body),
            });
            self.slots.len() - 1
        };
        self.len += 1;
        Ok(self.handle(index))
    }
    fn handle(&self, index: usize) -> BodyHandle {
        BodyHandle {
            index: index as u32,
            generation: self.slots[index].generation,
        }
    }
    pub fn body(&self, handle: BodyHandle) -> Option<&Body<D>> {
        let slot = self.slots.get(handle.index as usize)?;
        (slot.generation == handle.generation)
            .then_some(slot.body.as_ref())
            .flatten()
    }
    pub fn body_mut(&mut self, handle: BodyHandle) -> Option<&mut Body<D>> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        (slot.generation == handle.generation)
            .then_some(slot.body.as_mut())
            .flatten()
    }
    pub fn bodies(&self) -> impl Iterator<Item = (BodyHandle, &Body<D>)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.body.as_ref().map(|b| (self.handle(i), b)))
    }
    /// Removal invalidates the handle. Its outstanding contacts generate Stopped
    /// events on the next successful step, once each, including sensor contacts.
    pub fn remove(&mut self, handle: BodyHandle) -> Option<Body<D>> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        let body = slot.body.take()?;
        if slot.generation != u32::MAX {
            slot.generation += 1;
            self.free.push(handle.index as usize);
        }
        self.len -= 1;
        self.contacts.retain(|contact| {
            if contact.a == handle || contact.b == handle {
                self.pending_stops.push(ContactEvent {
                    kind: ContactEventKind::Stopped,
                    contact: *contact,
                });
                false
            } else {
                true
            }
        });
        Some(body)
    }
    pub fn contacts(&self) -> &[Contact<D>] {
        &self.contacts
    }
    /// Started/Stopped transitions from the last successful step. Persistent
    /// overlaps are available through `contacts`, not repeated Started events.
    pub fn events(&self) -> &[ContactEvent<D>] {
        &self.events
    }

    /// Advance exactly one timestep in (0, 0.1] seconds. Use a fixed dt, commonly
    /// 1/120, from the caller's fixed clock. Forces last for one successful step.
    pub fn step(&mut self, dt: f32) -> Result<StepStats, PhysicsError> {
        if !dt.is_finite() || dt <= 0. || dt > 0.1 {
            return Err(PhysicsError::InvalidTimestep);
        }
        self.backup.clear();
        for (index, slot) in self.slots.iter().enumerate() {
            if let Some(body) = &slot.body {
                self.backup.push(Motion {
                    index,
                    position: body.desc.position,
                    velocity: body.desc.velocity,
                });
            }
        }
        let result = self.advance(dt);
        if result.is_err() {
            for m in &self.backup {
                let b = self.slots[m.index].body.as_mut().unwrap();
                b.desc.position = m.position;
                b.desc.velocity = m.velocity;
            }
        }
        result
    }
    fn advance(&mut self, dt: f32) -> Result<StepStats, PhysicsError> {
        let mut stats = StepStats {
            bodies: self.len,
            ..StepStats::default()
        };
        for slot in &mut self.slots {
            let Some(body) = &mut slot.body else {
                continue;
            };
            if body.inv_mass() > 0. {
                let acceleration = add(
                    mul(self.config.gravity, body.desc.gravity_scale),
                    mul(body.force, body.inv_mass()),
                );
                body.desc.velocity = mul(
                    add(body.desc.velocity, mul(acceleration, dt)),
                    1. / (1. + body.desc.linear_damping * dt),
                );
            }
            if !valid(body.desc.velocity) {
                return Err(PhysicsError::NumericalLimit);
            }
            if body.desc.kind != BodyKind::Static
                && !(body.desc.ccd
                    && body.inv_mass() > 0.
                    && matches!(body.desc.shape, Shape::Ball { .. })
                    && !body.desc.sensor)
            {
                body.desc.position = add(body.desc.position, mul(body.desc.velocity, dt));
            }
        }
        for index in 0..self.slots.len() {
            let Some(body) = self.slots[index].body.as_ref() else {
                continue;
            };
            if !body.desc.ccd || body.desc.sensor || body.inv_mass() == 0. {
                continue;
            }
            let Shape::Ball { radius } = body.desc.shape else {
                continue;
            };
            let desc = body.desc;
            let mut p = desc.position;
            let mut v = desc.velocity;
            let mut remaining = dt;
            let skin = self.config.contact_slop * 0.2;
            for _ in 0..self.config.ccd_iterations {
                let displacement = mul(v, remaining);
                let distance = length(displacement);
                if distance < 1e-8 {
                    remaining = 0.;
                    break;
                }
                let direction = mul(displacement, distance.recip());
                let filter = QueryFilter {
                    exclude: Some(self.handle(index)),
                    layers: desc.layers,
                    only_static: true,
                    ..QueryFilter::default()
                };
                let Some(hit) = self.cast_ball(p, radius, direction, distance, filter) else {
                    p = add(p, displacement);
                    remaining = 0.;
                    break;
                };
                stats.ccd_impacts += 1;
                p = add(p, mul(direction, (hit.distance - skin).max(0.)));
                let target = self.body(hit.body).unwrap();
                let normal_speed = dot(v, hit.normal);
                if normal_speed < 0. {
                    let bounce = if normal_speed < -1. {
                        desc.restitution.max(target.desc.restitution)
                    } else {
                        0.
                    };
                    let normal_change = -(1. + bounce) * normal_speed;
                    v = add(v, mul(hit.normal, normal_change));
                    let tangent = sub(v, mul(hit.normal, dot(v, hit.normal)));
                    let tangent_speed = length(tangent);
                    if tangent_speed > 1e-8 {
                        let friction = (desc.friction * target.desc.friction).sqrt();
                        v = sub(
                            v,
                            mul(tangent, (friction * normal_change / tangent_speed).min(1.)),
                        );
                    }
                }
                remaining *= 1. - (hit.distance / distance).clamp(0., 1.);
                // A body initially embedded in terrain is left for the discrete
                // penetration solver; repeated zero-time impacts cannot loop.
                if hit.distance < skin && dot(v, hit.normal) <= 1e-6 {
                    break;
                }
            }
            if remaining > 1e-8 {
                stats.ccd_budget_stops += 1;
            }
            let body = self.slots[index].body.as_mut().unwrap();
            body.desc.position = p;
            body.desc.velocity = v;
        }
        if self
            .slots
            .iter()
            .filter_map(|s| s.body.as_ref())
            .any(|b| !valid(b.desc.position))
        {
            return Err(PhysicsError::NumericalLimit);
        }
        self.broadphase(&mut stats)?;
        let margin = self.config.contact_slop * 2.;
        // Sequential impulses plus iterative mass-weighted penetration correction.
        for _ in 0..self.config.solver_iterations {
            for &(a, b) in &self.pairs {
                let (left, right) = self.slots.split_at_mut(b);
                let a = left[a].body.as_mut().unwrap();
                let b = right[0].body.as_mut().unwrap();
                if a.desc.sensor || b.desc.sensor {
                    continue;
                }
                let Some(m) = contact(
                    a.desc.shape,
                    a.desc.position,
                    b.desc.shape,
                    b.desc.position,
                    margin,
                ) else {
                    continue;
                };
                solve(a, b, m, self.config.contact_slop);
            }
        }
        if self
            .slots
            .iter()
            .filter_map(|s| s.body.as_ref())
            .any(|b| !valid(b.desc.position) || !valid(b.desc.velocity))
        {
            return Err(PhysicsError::NumericalLimit);
        }
        self.next_contacts.clear();
        for &(a, b) in &self.pairs {
            let aa = self.slots[a].body.as_ref().unwrap();
            let bb = self.slots[b].body.as_ref().unwrap();
            if let Some(m) = contact(
                aa.desc.shape,
                aa.desc.position,
                bb.desc.shape,
                bb.desc.position,
                if aa.desc.sensor || bb.desc.sensor {
                    0.
                } else {
                    margin
                },
            ) {
                self.next_contacts.push(Contact {
                    a: self.handle(a),
                    b: self.handle(b),
                    normal: m.normal,
                    penetration: m.penetration,
                    sensor: aa.desc.sensor || bb.desc.sensor,
                });
            }
        }
        self.next_contacts.sort_unstable_by_key(|c| (c.a, c.b));
        self.events.clear();
        self.events.append(&mut self.pending_stops);
        let mut old = 0;
        let mut new = 0;
        while old < self.contacts.len() || new < self.next_contacts.len() {
            let a = self.contacts.get(old);
            let b = self.next_contacts.get(new);
            let order = match (a, b) {
                (Some(a), Some(b)) => (a.a, a.b).cmp(&(b.a, b.b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => unreachable!(),
            };
            match order {
                std::cmp::Ordering::Less => {
                    self.events.push(ContactEvent {
                        kind: ContactEventKind::Stopped,
                        contact: *a.unwrap(),
                    });
                    old += 1;
                }
                std::cmp::Ordering::Greater => {
                    self.events.push(ContactEvent {
                        kind: ContactEventKind::Started,
                        contact: *b.unwrap(),
                    });
                    new += 1;
                }
                std::cmp::Ordering::Equal => {
                    old += 1;
                    new += 1;
                }
            }
        }
        std::mem::swap(&mut self.contacts, &mut self.next_contacts);
        for body in self.slots.iter_mut().filter_map(|s| s.body.as_mut()) {
            body.clear_forces();
        }
        stats.contacts = self.contacts.len();
        Ok(stats)
    }
    fn broadphase(&mut self, stats: &mut StepStats) -> Result<(), PhysicsError> {
        self.proxies.clear();
        self.pairs.clear();
        let margin = self.config.contact_slop * 2.;
        for (index, slot) in self.slots.iter().enumerate() {
            if let Some(b) = &slot.body {
                let half = b.desc.shape.half().map(|v| v + margin);
                self.proxies.push(Proxy {
                    index,
                    min: sub(b.desc.position, half),
                    max: add(b.desc.position, half),
                });
            }
        }
        // Sweep along the axis with greatest center variance to avoid the common
        // degenerate case of a vertical stack whose every X interval overlaps.
        let mut sum = [0.; D];
        let mut squared = [0.; D];
        for p in &self.proxies {
            for axis in 0..D {
                let c = (p.min[axis] + p.max[axis]) * 0.5;
                sum[axis] += c;
                squared[axis] += c * c;
            }
        }
        let count = self.len.max(1) as f32;
        let axis = (0..D)
            .max_by(|&a, &b| {
                (squared[a] - sum[a] * sum[a] / count)
                    .total_cmp(&(squared[b] - sum[b] * sum[b] / count))
            })
            .unwrap();
        self.proxies.sort_unstable_by(|a, b| {
            a.min[axis]
                .total_cmp(&b.min[axis])
                .then(a.index.cmp(&b.index))
        });
        for i in 0..self.proxies.len() {
            let a = self.proxies[i];
            for b in &self.proxies[i + 1..] {
                if b.min[axis] > a.max[axis] {
                    break;
                }
                stats.broadphase_tests += 1;
                if stats.broadphase_tests > self.config.max_pair_tests {
                    return Err(PhysicsError::PairCapacity);
                }
                if (0..D).any(|axis| a.min[axis] > b.max[axis] || b.min[axis] > a.max[axis]) {
                    continue;
                }
                let aa = self.slots[a.index].body.as_ref().unwrap();
                let bb = self.slots[b.index].body.as_ref().unwrap();
                if !aa.desc.layers.allows(bb.desc.layers) {
                    continue;
                }
                if aa.desc.kind == BodyKind::Static
                    && bb.desc.kind == BodyKind::Static
                    && !aa.desc.sensor
                    && !bb.desc.sensor
                {
                    continue;
                }
                if self.pairs.len() == self.config.max_pairs {
                    return Err(PhysicsError::PairCapacity);
                }
                self.pairs
                    .push((a.index.min(b.index), a.index.max(b.index)));
            }
        }
        // Stable handle order makes solver order independent of the sweep axis.
        self.pairs.sort_unstable();
        stats.candidate_pairs = self.pairs.len();
        Ok(())
    }
    fn query_allows(&self, handle: BodyHandle, body: &Body<D>, filter: QueryFilter) -> bool {
        filter.exclude != Some(handle)
            && filter.layers.allows(body.desc.layers)
            && (filter.include_sensors || !body.desc.sensor)
            && (!filter.only_static || body.desc.kind == BodyKind::Static)
    }
    /// Nearest ray hit, reflecting current body transforms immediately. Direction
    /// is normalized, so `max_distance` and hit distances are in world units.
    pub fn raycast(
        &self,
        origin: [f32; D],
        direction: [f32; D],
        max_distance: f32,
        filter: QueryFilter,
    ) -> Option<RayHit<D>> {
        if !valid(origin)
            || !valid(direction)
            || !max_distance.is_finite()
            || !(0. ..=1_000_000.).contains(&max_distance)
        {
            return None;
        }
        let direction = normalize(direction)?;
        let mut max = max_distance;
        let mut result = None;
        for (handle, body) in self.bodies() {
            if !self.query_allows(handle, body, filter) {
                continue;
            }
            let hit = match body.desc.shape {
                Shape::Ball { radius } => {
                    ray_ball(origin, direction, body.desc.position, radius, max)
                }
                Shape::Box { half_extents } => {
                    ray_box(origin, direction, body.desc.position, half_extents, max)
                }
            };
            if let Some((distance, normal)) = hit {
                if distance == max && result.is_some() {
                    continue;
                }
                max = distance;
                result = Some(RayHit {
                    body: handle,
                    distance,
                    point: add(origin, mul(direction, distance)),
                    normal,
                });
            }
        }
        result
    }
    /// Cast a circle/sphere continuously. `point` is the moving ball's center at
    /// contact, not its surface point. Exact against boxes' rounded Minkowski
    /// boundaries. Sensors are excluded by default; filtering is explicit.
    pub fn cast_ball(
        &self,
        origin: [f32; D],
        radius: f32,
        direction: [f32; D],
        max_distance: f32,
        filter: QueryFilter,
    ) -> Option<RayHit<D>> {
        if !valid(origin)
            || !valid(direction)
            || !Shape::<D>::ball(radius).valid()
            || !max_distance.is_finite()
            || !(0. ..=1_000_000.).contains(&max_distance)
        {
            return None;
        }
        let direction = normalize(direction)?;
        let mut max = max_distance;
        let mut result = None;
        for (handle, body) in self.bodies() {
            if !self.query_allows(handle, body, filter) {
                continue;
            }
            let hit = match body.desc.shape {
                Shape::Ball {
                    radius: target_radius,
                } => {
                    let hit = ray_ball(
                        origin,
                        direction,
                        body.desc.position,
                        radius + target_radius,
                        max,
                    );
                    hit.filter(|(distance, normal)| {
                        *distance > 1e-6
                            || dot(direction, *normal) < 0.
                            || length(sub(origin, body.desc.position))
                                < radius + target_radius - 1e-5
                    })
                }
                Shape::Box { half_extents } => sweep_ball_box(
                    origin,
                    radius,
                    direction,
                    body.desc.position,
                    half_extents,
                    max,
                ),
            };
            if let Some((distance, normal)) = hit {
                if distance == max && result.is_some() {
                    continue;
                }
                max = distance;
                result = Some(RayHit {
                    body: handle,
                    distance,
                    point: add(origin, mul(direction, distance)),
                    normal,
                });
            }
        }
        result
    }
}

fn solve<const D: usize>(a: &mut Body<D>, b: &mut Body<D>, m: Manifold<D>, slop: f32) {
    let ma = a.inv_mass();
    let mb = b.inv_mass();
    let inverse_mass = ma + mb;
    if inverse_mass == 0. {
        return;
    }
    let relative = sub(b.desc.velocity, a.desc.velocity);
    let speed = dot(relative, m.normal);
    if speed < 0. {
        // Low-speed contacts must not bounce forever under gravity.
        let restitution = if speed < -1. {
            a.desc.restitution.max(b.desc.restitution)
        } else {
            0.
        };
        let impulse = -(1. + restitution) * speed / inverse_mass;
        a.desc.velocity = sub(a.desc.velocity, mul(m.normal, impulse * ma));
        b.desc.velocity = add(b.desc.velocity, mul(m.normal, impulse * mb));
        let tangent = sub(relative, mul(m.normal, speed));
        let tangent_speed = length(tangent);
        if tangent_speed > 1e-8 {
            let friction = (a.desc.friction * b.desc.friction).sqrt();
            let friction_impulse = (tangent_speed / inverse_mass).min(friction * impulse);
            let friction_vector = mul(tangent, friction_impulse / tangent_speed);
            a.desc.velocity = add(a.desc.velocity, mul(friction_vector, ma));
            b.desc.velocity = sub(b.desc.velocity, mul(friction_vector, mb));
        }
    }
    let correction = (m.penetration - slop).max(0.) * 0.75 / inverse_mass;
    a.desc.position = sub(a.desc.position, mul(m.normal, correction * ma));
    b.desc.position = add(b.desc.position, mul(m.normal, correction * mb));
}
