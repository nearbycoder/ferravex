use super::*;
#[derive(Clone, Copy)]
struct Flags {
    awake: bool,
    asleep: bool,
    request: bool,
    quiet: bool,
    anchored: bool,
    gravity: bool,
    time: f32,
    count: usize,
}
impl Default for Flags {
    fn default() -> Self {
        Self {
            awake: false,
            asleep: false,
            request: false,
            quiet: true,
            anchored: false,
            gravity: false,
            time: f32::INFINITY,
            count: 0,
        }
    }
}
#[derive(Default)]
pub(super) struct Islands {
    parents: Vec<usize>,
    flags: Vec<Flags>,
}
impl Islands {
    fn reset<B: Access>(&mut self, bodies: &BodySet<B>) {
        let len = bodies.iter().map(|(h, _)| h.index + 1).max().unwrap_or(0);
        self.parents.resize(len, usize::MAX);
        self.parents.fill(usize::MAX);
        self.flags.resize(len, Flags::default());
        self.flags.fill(Flags::default());
        for (id, b) in bodies.iter() {
            if b.core().kind == Kind::Dynamic {
                self.parents[id.index] = id.index;
            }
        }
    }
    fn root(&mut self, mut i: usize) -> usize {
        while self.parents[i] != i {
            self.parents[i] = self.parents[self.parents[i]];
            i = self.parents[i];
        }
        i
    }
    fn join(&mut self, a: RigidBodyHandle, b: RigidBodyHandle) {
        if self.parents[a.index] == usize::MAX || self.parents[b.index] == usize::MAX {
            return;
        }
        let a = self.root(a.index);
        let b = self.root(b.index);
        self.parents[a.max(b)] = a.min(b);
    }
    fn seed<B: Access>(&mut self, bodies: &BodySet<B>, force: bool) {
        for (id, b) in bodies.iter() {
            let b = b.core();
            if b.kind != Kind::Dynamic {
                continue;
            }
            let root = self.root(id.index);
            let f = &mut self.flags[root];
            f.awake |= !b.sleeping;
            f.asleep |= b.sleeping;
            f.request |= force || b.wake_requested || b.solver_dirty;
        }
    }
    fn request(&mut self, id: RigidBodyHandle) {
        if self.parents[id.index] != usize::MAX {
            let root = self.root(id.index);
            self.flags[root].request = true;
        }
    }
    fn activate<B: Access>(&mut self, bodies: &mut BodySet<B>) {
        for (id, b) in bodies.iter_mut() {
            let b = b.core_mut();
            if b.kind != Kind::Dynamic {
                continue;
            }
            let root = self.root(id.index);
            let f = self.flags[root];
            if f.request || f.awake && f.asleep {
                b.wake_internal();
            }
        }
    }
}
fn moving(b: &crate::body::Core) -> bool {
    b.kind == Kind::Kinematic
        && b.velocity.length_squared() + b.angular_velocity.length_squared() > 1e-12
}
impl<B: Access> World<B> {
    fn join_joint_islands(&mut self) {
        for (_, j) in self.joints.iter() {
            if j.desc.enabled {
                self.islands.join(j.a, j.b);
            }
        }
    }
    fn request_joint_wakes(&mut self) {
        for (_, j) in self.joints.iter() {
            if !j.desc.enabled {
                continue;
            }
            let a = self.bodies[j.a].core();
            let b = self.bodies[j.b].core();
            if a.solver_dirty
                || b.solver_dirty
                || moving(a)
                || moving(b)
                || super::joints::motor_active(j, a, b)
            {
                self.islands.request(j.a);
                self.islands.request(j.b);
            }
        }
    }
    pub(super) fn wake_previous_islands(&mut self, gravity_changed: bool) {
        self.islands.reset(&self.bodies);
        for c in &self.contacts {
            if c.sensor {
                continue;
            }
            if let (Some(a), Some(b)) = (
                self.colliders.get(c.collider1),
                self.colliders.get(c.collider2),
            ) {
                self.islands.join(a.parent.unwrap(), b.parent.unwrap());
            }
        }
        self.join_joint_islands();
        self.islands
            .seed(&self.bodies, gravity_changed || !self.settings.allow_sleep);
        self.request_joint_wakes();
        for c in &self.contacts {
            if c.sensor {
                continue;
            }
            if let (Some(ca), Some(cb)) = (
                self.colliders.get(c.collider1),
                self.colliders.get(c.collider2),
            ) {
                let a = ca.parent.unwrap();
                let b = cb.parent.unwrap();
                let aa = self.bodies[a].core();
                let bb = self.bodies[b].core();
                if ca.dirty
                    || cb.dirty
                    || aa.solver_dirty
                    || bb.solver_dirty
                    || moving(aa)
                    || moving(bb)
                {
                    self.islands.request(a);
                    self.islands.request(b);
                }
            }
        }
        self.islands.activate(&mut self.bodies);
    }
    pub(super) fn wake_contact_islands(&mut self) {
        self.islands.reset(&self.bodies);
        for c in &self.constraints {
            self.islands.join(c.a, c.b);
        }
        self.join_joint_islands();
        self.islands.seed(&self.bodies, !self.settings.allow_sleep);
        self.request_joint_wakes();
        for c in &self.constraints {
            if moving(self.bodies[c.a].core()) || moving(self.bodies[c.b].core()) {
                self.islands.request(c.a);
                self.islands.request(c.b);
            }
        }
        self.islands.activate(&mut self.bodies);
    }
    pub(super) fn sleep_islands(&mut self, dt: f32, stats: &mut StepStats) {
        self.islands.flags.fill(Flags::default());
        for (id, body) in self.bodies.iter() {
            let b = body.core();
            if b.kind != Kind::Dynamic {
                continue;
            }
            let root = self.islands.root(id.index);
            let f = &mut self.islands.flags[root];
            f.count += 1;
            f.time = f.time.min(b.quiet);
            f.quiet &= b.velocity.length_squared() < self.settings.linear_sleep_threshold.powi(2)
                && b.angular_velocity.length_squared()
                    < self.settings.angular_sleep_threshold.powi(2)
                && b.force.length_squared() + b.torque.length_squared() < 1e-12;
            f.gravity |= (self.gravity * b.gravity_scale).length_squared() > 1e-12;
        }
        for c in &self.constraints {
            for (id, other, normal) in [(c.a, c.b, -c.normal), (c.b, c.a, c.normal)] {
                let b = self.bodies[id].core();
                if b.kind != Kind::Dynamic {
                    continue;
                }
                let other = self.bodies[other].core();
                let root = self.islands.root(id.index);
                let f = &mut self.islands.flags[root];
                if moving(other) {
                    f.quiet = false;
                }
                if other.kind != Kind::Dynamic
                    && normal.dot(-(self.gravity * b.gravity_scale).normalize_or_zero()) > 0.2
                {
                    f.anchored = true;
                }
            }
        }
        for (_, j) in self.joints.iter() {
            if !j.desc.enabled {
                continue;
            }
            let aa = self.bodies[j.a].core();
            let bb = self.bodies[j.b].core();
            let motor = super::joints::motor_active(j, aa, bb);
            for (id, other) in [(j.a, bb), (j.b, aa)] {
                if self.bodies[id].core().kind != Kind::Dynamic {
                    continue;
                }
                let root = self.islands.root(id.index);
                let f = &mut self.islands.flags[root];
                if other.kind != Kind::Dynamic {
                    f.anchored = true;
                }
                if moving(other) || motor {
                    f.quiet = false;
                }
            }
        }
        stats.islands = 0;
        stats.active_islands = 0;
        stats.sleeping_islands = 0;
        for f in &mut self.islands.flags {
            if f.count == 0 {
                continue;
            }
            stats.islands += 1;
            f.time = if self.settings.allow_sleep && f.quiet && (!f.gravity || f.anchored) {
                f.time + dt
            } else {
                0.
            };
            if f.time >= self.settings.sleep_delay {
                stats.sleeping_islands += 1;
            } else {
                stats.active_islands += 1;
            }
        }
        for (id, body) in self.bodies.iter_mut() {
            let b = body.core_mut();
            if b.kind != Kind::Dynamic {
                continue;
            }
            let root = self.islands.root(id.index);
            let f = self.islands.flags[root];
            b.quiet = f.time;
            b.sleeping = f.time >= self.settings.sleep_delay;
            if b.sleeping {
                b.velocity = Vec3::ZERO;
                b.angular_velocity = Vec3::ZERO;
            }
        }
    }
}
