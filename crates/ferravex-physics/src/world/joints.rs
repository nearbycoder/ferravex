use super::*;
use crate::{
    JointDesc, JointHandle, JointKind, PhysicsError,
    joints::{CachedRow, Joint, MAX_JOINTS},
};
#[derive(Clone, Copy)]
pub(super) enum JointPhase {
    Refresh,
    Prepare,
    Velocity,
    Position,
    Integrate,
    Store,
}
#[derive(Clone, Copy)]
pub(super) struct JointRow {
    id: JointHandle,
    slot: usize,
    a: RigidBodyHandle,
    b: RigidBodyHandle,
    linear: Vec3,
    angular_a: Vec3,
    angular_b: Vec3,
    error: f32,
    bias: f32,
    gamma: f32,
    lower: f32,
    upper: f32,
    impulse: f32,
    spring: bool,
}
impl JointRow {
    fn denominator(self, a: &crate::body::Core, b: &crate::body::Core) -> f32 {
        (a.contact_inv_mass() + b.contact_inv_mass()) * self.linear.length_squared()
            + self
                .angular_a
                .dot(a.contact_inverse_inertia(self.angular_a))
            + self
                .angular_b
                .dot(b.contact_inverse_inertia(self.angular_b))
    }
    fn velocity(self, a: &crate::body::Core, b: &crate::body::Core) -> f32 {
        self.linear.dot(b.velocity - a.velocity)
            + self.angular_a.dot(a.angular_velocity)
            + self.angular_b.dot(b.angular_velocity)
    }
    fn apply(self, a: &mut crate::body::Core, b: &mut crate::body::Core, impulse: f32) {
        a.velocity -= self.linear * (a.contact_inv_mass() * impulse);
        b.velocity += self.linear * (b.contact_inv_mass() * impulse);
        a.angular_velocity += a.contact_inverse_inertia(self.angular_a * impulse);
        b.angular_velocity += b.contact_inverse_inertia(self.angular_b * impulse);
    }
    fn axis(self) -> Vec3 {
        if self.linear.length_squared() > 0. {
            self.linear
        } else {
            self.angular_b
        }
    }
}

fn body_pair(a: RigidBodyHandle, b: RigidBodyHandle) -> (RigidBodyHandle, RigidBodyHandle) {
    if a < b { (a, b) } else { (b, a) }
}
fn twist(j: &Joint, a: &crate::body::Core, b: &crate::body::Core) -> f32 {
    let JointKind::Hinge { axis1, .. } = j.desc.kind else {
        return 0.;
    };
    let delta =
        a.rotation.conjugate() * b.rotation * j.desc.reference_rotation.unwrap().conjugate();
    let angle = 2.
        * Vec3::new(delta.x, delta.y, delta.z)
            .dot(axis1)
            .atan2(delta.w);
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
fn target(j: &Joint, angle: f32) -> f32 {
    let Some(m) = j.desc.motor else {
        return 0.;
    };
    let position = m
        .target_position
        .map(|p| j.desc.limits.map_or(p, |[lo, hi]| p.clamp(lo, hi)));
    (m.target_velocity + position.map_or(0., |p| (p - angle) * m.stiffness))
        .clamp(-m.max_speed, m.max_speed)
}
pub(super) fn motor_speed(j: &Joint, a: &crate::body::Core, b: &crate::body::Core) -> f32 {
    if j.desc.enabled && j.desc.motor.is_some_and(|m| m.max_torque > 0.) {
        target(j, twist(j, a, b)).abs()
    } else {
        0.
    }
}
pub(super) fn motor_active(j: &Joint, a: &crate::body::Core, b: &crate::body::Core) -> bool {
    j.desc.enabled
        && j.desc
            .motor
            .is_some_and(|m| m.max_torque > 0. && target(j, twist(j, a, b)).abs() > 0.01)
}
/// Stable slots let rows preserve impulses without allocations. The largest 3D
/// hinge has 3 anchor rows, 2 alignment rows, a limit and a motor.
fn rows(
    id: JointHandle,
    j: &Joint,
    a: &crate::body::Core,
    b: &crate::body::Core,
    dt: f32,
    mut emit: impl FnMut(JointRow),
) {
    let pa = a.position + a.rotation * j.desc.local_anchor1;
    let pb = b.position + b.rotation * j.desc.local_anchor2;
    let ra = a.rotation * (j.desc.local_anchor1 - a.local_com);
    let rb = b.rotation * (j.desc.local_anchor2 - b.local_com);
    let delta = pb - pa;
    let point = |slot, n: Vec3, error| JointRow {
        id,
        slot,
        a: j.a,
        b: j.b,
        linear: n,
        angular_a: ra.cross(-n),
        angular_b: rb.cross(n),
        error,
        bias: 0.,
        gamma: 0.,
        lower: f32::NEG_INFINITY,
        upper: f32::INFINITY,
        impulse: 0.,
        spring: false,
    };
    let angular = |slot, n: Vec3, error| JointRow {
        id,
        slot,
        a: j.a,
        b: j.b,
        linear: Vec3::ZERO,
        angular_a: -n,
        angular_b: n,
        error,
        bias: 0.,
        gamma: 0.,
        lower: f32::NEG_INFINITY,
        upper: f32::INFINITY,
        impulse: 0.,
        spring: false,
    };
    match j.desc.kind {
        JointKind::Distance { length } | JointKind::Spring { length, .. } => {
            let n = delta.try_normalize().unwrap_or(Vec3::X);
            let mut row = point(0, n, delta.length() - length);
            if let JointKind::Spring {
                stiffness, damping, ..
            } = j.desc.kind
            {
                row.gamma = 1. / (dt * (damping + dt * stiffness));
                // Positions were predicted before the contact phase. Recover the
                // start-of-substep extension for the implicit spring equation,
                // then apply the spring's displacement after its velocity solve.
                row.bias = (row.error - dt * row.velocity(a, b)) * dt * stiffness * row.gamma;
                row.spring = true;
            }
            emit(row);
        }
        JointKind::Fixed | JointKind::Hinge { .. } => {
            for (i, n) in [Vec3::X, Vec3::Y, Vec3::Z]
                .into_iter()
                .enumerate()
                .take(if a.planar { 2 } else { 3 })
            {
                emit(point(i, n, delta.dot(n)));
            }
            if matches!(j.desc.kind, JointKind::Fixed) {
                let error =
                    rotation_delta(a.rotation * j.desc.reference_rotation.unwrap(), b.rotation);
                for (i, n) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                    if !a.planar || i == 2 {
                        emit(angular(3 + i, n, error.dot(n)));
                    }
                }
            } else if let JointKind::Hinge { axis1, axis2 } = j.desc.kind {
                let axis = a.rotation * axis1;
                if !a.planar {
                    let other = b.rotation * axis2;
                    let basis = axis
                        .cross(if axis.x.abs() < 0.8 { Vec3::X } else { Vec3::Y })
                        .normalize();
                    let mut error = axis.cross(other);
                    if axis.dot(other) < -0.999 {
                        error = basis * std::f32::consts::PI;
                    }
                    for (i, n) in [basis, axis.cross(basis)].into_iter().enumerate() {
                        emit(angular(3 + i, n, error.dot(n)));
                    }
                }
                let angle = twist(j, a, b);
                if let Some([lo, hi]) = j.desc.limits {
                    if angle <= lo {
                        let mut row = angular(6, axis, angle - lo);
                        row.lower = 0.;
                        emit(row);
                    } else if angle >= hi {
                        let mut row = angular(6, axis, angle - hi);
                        row.upper = 0.;
                        emit(row);
                    }
                }
                if let Some(m) = j.desc.motor {
                    let mut row = angular(7, axis, 0.);
                    row.bias = -target(j, angle);
                    row.lower = -m.max_torque * dt;
                    row.upper = m.max_torque * dt;
                    emit(row);
                }
            }
        }
    }
}
impl<B: Access> World<B> {
    // Shared dimensional authoring path. This only samples live authored body
    // origins; neither collider poses nor compound centers of mass define frames.
    pub(crate) fn joint_desc_from_world(
        &self,
        a: RigidBodyHandle,
        b: RigidBodyHandle,
        mut desc: JointDesc,
        anchors: [Vec3; 2],
        axis: Vec3,
        preserve_pose: bool,
    ) -> Result<JointDesc, PhysicsError> {
        let (Some(aa), Some(bb)) = (self.bodies.get(a), self.bodies.get(b)) else {
            return Err(PhysicsError::InvalidHandle);
        };
        let (aa, bb) = (aa.core(), bb.core());
        if a == b || aa.kind != Kind::Dynamic && bb.kind != Kind::Dynamic {
            return Err(PhysicsError::InvalidJoint);
        }
        if [aa, bb].into_iter().any(|body| {
            !body.position.is_finite()
                || !body.rotation.is_finite()
                || !body.rotation.is_normalized()
        }) {
            return Err(PhysicsError::InvalidBody);
        }
        desc.local_anchor1 = aa.rotation.conjugate() * (anchors[0] - aa.position);
        desc.local_anchor2 = bb.rotation.conjugate() * (anchors[1] - bb.position);
        if matches!(desc.kind, JointKind::Hinge { .. }) {
            // Normalize before rotating so huge/subnormal finite world axes
            // take the math kernel's checked path without overflow/underflow.
            let axis = axis.try_normalize().ok_or(PhysicsError::InvalidJoint)?;
            desc.kind = JointKind::Hinge {
                axis1: aa.rotation.conjugate() * axis,
                axis2: bb.rotation.conjugate() * axis,
            };
        }
        if preserve_pose || desc.reference_rotation.is_none() {
            desc.reference_rotation = Some(
                (aa.rotation.conjugate() * bb.rotation)
                    .try_normalize()
                    .ok_or(PhysicsError::InvalidBody)?,
            );
        }
        if preserve_pose {
            match &mut desc.kind {
                JointKind::Distance { length } | JointKind::Spring { length, .. } => {
                    *length = anchors[0].distance(anchors[1]);
                }
                JointKind::Fixed | JointKind::Hinge { .. } => {}
            }
        }
        desc.validate(self.planar)
            .map_err(|_| PhysicsError::InvalidJoint)?;
        // Separated fixed/hinge pivots cannot retain the current pose. Reject
        // rather than silently providing a mode with an initial anchor error.
        if preserve_pose
            && matches!(desc.kind, JointKind::Fixed | JointKind::Hinge { .. })
            && anchors[0] != anchors[1]
        {
            return Err(PhysicsError::InvalidJoint);
        }
        Ok(desc)
    }
    // Install the implementation only from insert_joint. Worlds that never use
    // connections retain a cheap None check, and linkers can strip joint rows.
    pub(super) fn run_joints(&mut self, phase: JointPhase, dt: f32, stats: &mut StepStats) {
        if let Some(solver) = self.joint_solver {
            solver(self, phase, dt, stats);
        }
    }
    fn dispatch_joints(&mut self, phase: JointPhase, dt: f32, stats: &mut StepStats) {
        match phase {
            JointPhase::Refresh => self.refresh_joints(),
            JointPhase::Prepare => self.prepare_joint_rows(dt, stats),
            JointPhase::Velocity => self.solve_joint_velocities(stats),
            JointPhase::Position => self.solve_joint_positions(),
            JointPhase::Integrate => self.integrate_spring_impulses(dt),
            JointPhase::Store => self.store_joint_impulses(dt),
        }
    }
    // Endpoints are staged bodies, so no live handles or solver changes are
    // required to apply the same descriptor/type rules as insert_joint.
    pub(crate) fn check_assembly_joint(
        &self,
        a: &B,
        b: &B,
        same_body: bool,
        desc: JointDesc,
    ) -> Result<(), PhysicsError> {
        if same_body
            || !desc.valid(self.planar)
            || a.core().kind != Kind::Dynamic && b.core().kind != Kind::Dynamic
        {
            return Err(PhysicsError::InvalidJoint);
        }
        Ok(())
    }

    pub fn insert_joint(
        &mut self,
        a: RigidBodyHandle,
        b: RigidBodyHandle,
        desc: impl Into<JointDesc>,
    ) -> Result<JointHandle, PhysicsError> {
        let (Some(aa), Some(bb)) = (self.bodies.get(a), self.bodies.get(b)) else {
            return Err(PhysicsError::InvalidHandle);
        };
        let (aa, bb) = (aa.core(), bb.core());
        let mut desc = desc.into();
        if a == b
            || !desc.valid(self.planar)
            || aa.kind != Kind::Dynamic && bb.kind != Kind::Dynamic
        {
            return Err(PhysicsError::InvalidJoint);
        }
        if self.joints.len() >= MAX_JOINTS {
            return Err(PhysicsError::JointCapacity);
        }
        if desc.reference_rotation.is_none() {
            desc.reference_rotation = Some(aa.rotation.conjugate() * bb.rotation);
        }
        self.broken_joints.reserve(self.joints.len() + 1);
        self.joint_solver = Some(Self::dispatch_joints);
        let id = self.joints.insert(Joint {
            a,
            b,
            desc,
            dirty: true,
            revisions: [aa.revision, bb.revision],
            dt: 0.,
            cache: [CachedRow::default(); 8],
            load: crate::JointLoad::default(),
            peak: crate::JointLoad::default(),
        });
        self.refresh_joints();
        Ok(id)
    }
    pub fn remove_joint(&mut self, id: JointHandle) -> bool {
        let Some(j) = self.joints.remove(id) else {
            return false;
        };
        for body in [j.a, j.b] {
            if let Some(b) = self.bodies.get_mut(body) {
                b.core_mut().invalidate_contacts();
                b.core_mut().wake();
            }
        }
        self.rebuild_joint_filters();
        true
    }
    pub fn joint_angle(&self, id: JointHandle) -> Option<f32> {
        let j = self.joints.get(id)?;
        matches!(j.desc.kind, JointKind::Hinge { .. })
            .then(|| twist(j, self.bodies[j.a].core(), self.bodies[j.b].core()))
    }
    pub fn joint_anchors(&self, id: JointHandle) -> Option<[Vec3; 2]> {
        let j = self.joints.get(id)?;
        let a = self.bodies[j.a].core();
        let b = self.bodies[j.b].core();
        Some([
            a.position + a.rotation * j.desc.local_anchor1,
            b.position + b.rotation * j.desc.local_anchor2,
        ])
    }
    pub(super) fn joint_contacts_disabled(&self, a: RigidBodyHandle, b: RigidBodyHandle) -> bool {
        self.disabled_joint_pairs
            .binary_search(&body_pair(a, b))
            .is_ok()
    }
    fn rebuild_joint_filters(&mut self) {
        self.disabled_joint_pairs.clear();
        self.disabled_joint_pairs.extend(
            self.joints
                .iter()
                .filter(|(_, j)| j.desc.enabled && !j.desc.contacts_enabled)
                .map(|(_, j)| body_pair(j.a, j.b)),
        );
        self.disabled_joint_pairs.sort_unstable();
        self.disabled_joint_pairs.dedup();
    }
    pub(super) fn refresh_joints(&mut self) {
        let mut dirty = false;
        for (_, j) in self.joints.iter_mut() {
            let revisions = [
                self.bodies[j.a].core().revision,
                self.bodies[j.b].core().revision,
            ];
            if j.dirty || j.revisions != revisions {
                if j.dirty {
                    for id in [j.a, j.b] {
                        let b = self.bodies[id].core_mut();
                        b.invalidate_contacts();
                        b.wake();
                    }
                    dirty = true;
                }
                j.cache.fill(CachedRow::default());
                j.dt = 0.;
                j.dirty = false;
                j.revisions = [
                    self.bodies[j.a].core().revision,
                    self.bodies[j.b].core().revision,
                ];
            }
        }
        if dirty {
            self.rebuild_joint_filters();
        }
    }
    pub(super) fn prepare_joint_rows(&mut self, dt: f32, stats: &mut StepStats) {
        self.joint_rows.clear();
        for (id, j) in self.joints.iter() {
            if !j.desc.enabled {
                continue;
            }
            let a = self.bodies[j.a].core();
            let b = self.bodies[j.b].core();
            if a.contact_inv_mass() + b.contact_inv_mass() == 0. {
                continue;
            }
            let ratio = dt / j.dt;
            rows(id, j, a, b, dt, |mut row| {
                if self.settings.warm_start
                    && (0.5..=2.).contains(&ratio)
                    && row.axis().dot(j.cache[row.slot].axis) > 0.95
                {
                    row.impulse = (j.cache[row.slot].impulse * ratio).clamp(row.lower, row.upper);
                }
                self.joint_rows.push(row);
            });
        }
        stats.joint_rows += self.joint_rows.len();
        for row in &self.joint_rows {
            let (a, b) = self.bodies.two_mut(row.a, row.b);
            row.apply(a.core_mut(), b.core_mut(), row.impulse);
        }
    }
    pub(super) fn solve_joint_velocities(&mut self, stats: &mut StepStats) {
        for row in &mut self.joint_rows {
            let (a, b) = self.bodies.two_mut(row.a, row.b);
            let (a, b) = (a.core_mut(), b.core_mut());
            let denom = row.denominator(a, b) + row.gamma;
            if denom <= 1e-10 {
                continue;
            }
            let delta = -(row.velocity(a, b) + row.bias + row.gamma * row.impulse) / denom;
            let accumulated = (row.impulse + delta).clamp(row.lower, row.upper);
            row.apply(a, b, accumulated - row.impulse);
            row.impulse = accumulated;
            stats.joint_constraints += 1;
        }
    }
    pub(super) fn integrate_spring_impulses(&mut self, dt: f32) {
        for row in &self.joint_rows {
            if row.spring {
                let (a, b) = self.bodies.two_mut(row.a, row.b);
                let impulse = row.impulse * dt;
                a.core_mut()
                    .position_impulse(-row.linear * impulse, row.angular_a * impulse);
                b.core_mut()
                    .position_impulse(row.linear * impulse, row.angular_b * impulse);
            }
        }
    }
    pub(super) fn solve_joint_positions(&mut self) {
        for (id, j) in self.joints.iter() {
            if !j.desc.enabled || matches!(j.desc.kind, JointKind::Spring { .. }) {
                continue;
            }
            let (a, b) = self.bodies.two_mut(j.a, j.b);
            let (a, b) = (a.core_mut(), b.core_mut());
            // Rebuild against the corrected poses each pass without a heap buffer.
            let mut buffer = [None; 8];
            rows(id, j, a, b, 1., |row| {
                buffer[row.slot] = Some(row);
            });
            for row in buffer.into_iter().flatten().filter(|r| r.slot != 7) {
                let denom = row.denominator(a, b);
                if denom <= 1e-10 {
                    continue;
                }
                let impulse =
                    (-row.error.clamp(-0.2, 0.2) * 0.6 / denom).clamp(row.lower, row.upper);
                a.position_impulse(-row.linear * impulse, row.angular_a * impulse);
                b.position_impulse(row.linear * impulse, row.angular_b * impulse);
            }
        }
    }
    pub(super) fn store_joint_impulses(&mut self, dt: f32) {
        for (_, j) in self.joints.iter_mut() {
            j.cache.fill(CachedRow::default());
            j.dt = dt;
            j.load = crate::JointLoad::default();
        }
        for row in &self.joint_rows {
            let load = &mut self.joints[row.id].load;
            load.force += row.impulse.abs() * row.linear.length() / dt;
            load.torque +=
                row.impulse.abs() * row.angular_a.length().max(row.angular_b.length()) / dt;
            self.joints[row.id].cache[row.slot] = CachedRow {
                impulse: row.impulse,
                axis: row.axis(),
            };
        }
        let first = self.broken_joints.len();
        for (id, j) in self.joints.iter_mut() {
            j.peak.force = j.peak.force.max(j.load.force);
            j.peak.torque = j.peak.torque.max(j.load.torque);
            if j.desc.enabled
                && j.desc
                    .break_limits
                    .is_some_and(|l| j.load.force > l.force || j.load.torque > l.torque)
            {
                self.broken_joints.push(crate::JointBreakEvent {
                    joint: id,
                    bodies: [j.a, j.b],
                    load: j.load,
                });
            }
        }
        for event in &self.broken_joints[first..] {
            self.joints.remove(event.joint);
            for id in event.bodies {
                if let Some(b) = self.bodies.get_mut(id) {
                    b.core_mut().invalidate_contacts();
                    b.core_mut().wake();
                }
            }
        }
        if self.broken_joints.len() > first {
            self.rebuild_joint_filters();
        }
    }
}

#[cfg(test)]
mod journey400_223 {
    use super::*;

    macro_rules! current_pose_tests {
        ($module:ident) => {
            mod $module {
                use super::*;
                use crate::$module::{
                    ColliderBuilder, JointBuilder, JointPoseMode, PhysicsWorld, RigidBodyBuilder,
                    Vector, WorldJointFrame,
                };

                #[test]
                fn invalid_retained_pose_is_rejected_without_mutating_either_body() {
                    let mut world = PhysicsWorld::default();
                    let a = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                    let (b, _) = world.insert(
                        RigidBodyBuilder::dynamic().translation(Vector::X),
                        ColliderBuilder::ball(0.2),
                    );
                    let original = world.bodies[b].core().clone();
                    for (position, rotation) in [
                        (Vec3::NAN, Quat::IDENTITY),
                        (Vec3::INFINITY, Quat::IDENTITY),
                        (Vec3::X, Quat::NAN),
                        (Vec3::X, Quat::from_xyzw(0., 0., 0., 0.)),
                        (Vec3::X, Quat::IDENTITY * 2.),
                    ] {
                        for id in [a, b] {
                            let saved = world.bodies[id].core().clone();
                            let body = world.bodies[id].core_mut();
                            body.position = position;
                            body.rotation = rotation;
                            let before = format!("{:?}", world.bodies);
                            assert_eq!(
                                world.insert_joint_from_world(
                                    a,
                                    b,
                                    JointBuilder::fixed(),
                                    WorldJointFrame::at_pivot(Vector::ZERO),
                                    JointPoseMode::Preserve,
                                ),
                                Err(PhysicsError::InvalidBody)
                            );
                            assert_eq!(format!("{:?}", world.bodies), before);
                            assert!(world.joints.is_empty());
                            *world.bodies[id].core_mut() = saved;
                        }
                    }
                    assert_eq!(
                        format!("{:?}", world.bodies[b].core()),
                        format!("{original:?}")
                    );
                    // Finite inputs can overflow during inverse translation.
                    // Reject the derived local frame rather than passing it on.
                    world.bodies[a].core_mut().position = Vec3::X * f32::MAX;
                    world.bodies[b].core_mut().position = -Vec3::X * f32::MAX;
                    assert_eq!(
                        world.joint_desc_from_world(
                            a,
                            b,
                            JointBuilder::fixed(),
                            WorldJointFrame::at_pivot(Vector::X * f32::MAX),
                            JointPoseMode::Preserve,
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                }

                #[test]
                fn sampling_and_rejection_preserve_live_joint_caches_and_pending_motion() {
                    let mut world = PhysicsWorld::default();
                    let a = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                    let (b, _) = world.insert(
                        RigidBodyBuilder::dynamic()
                            .translation(Vector::X)
                            .linvel(Vector::X),
                        ColliderBuilder::ball(0.2),
                    );
                    let existing = world
                        .insert_joint(a, b, JointBuilder::distance(1.))
                        .unwrap();
                    world.step(1. / 60.);
                    assert!(
                        world.joints[existing]
                            .cache
                            .iter()
                            .any(|row| row.impulse != 0.)
                    );
                    let body = world.bodies[b].core_mut();
                    body.force = Vec3::Y;
                    body.torque = Vec3::Z;
                    body.next_position = Some(Vec3::X * 5.);
                    body.next_rotation = Some(Quat::from_rotation_z(0.3));
                    // Read-only authoring requires no revision budget.
                    body.revision = u64::MAX;
                    let before =
                        format!("{:?}{:?}{:?}", world.bodies, world.colliders, world.joints);
                    let desc = world
                        .joint_desc_from_world(
                            a,
                            b,
                            JointBuilder::hinge(),
                            WorldJointFrame::at_pivot(Vector::ZERO),
                            JointPoseMode::Preserve,
                        )
                        .unwrap();
                    desc.validate(world.planar).unwrap();
                    assert_eq!(
                        world.insert_joint_from_world(
                            a,
                            b,
                            JointBuilder::hinge().limits(1., -1.),
                            WorldJointFrame::at_pivot(Vector::ZERO),
                            JointPoseMode::Preserve,
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                    assert_eq!(
                        format!("{:?}{:?}{:?}", world.bodies, world.colliders, world.joints),
                        before
                    );
                }
            }
        };
    }
    #[cfg(feature = "dim2")]
    current_pose_tests!(d2);
    #[cfg(feature = "dim3")]
    current_pose_tests!(d3);
}

#[cfg(all(test, feature = "dim2"))]
mod journey400_224 {
    use super::*;
    use crate::assembly::{Assembly, AssemblyError};
    use crate::d2::{ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder};

    #[test]
    fn late_error_does_not_refresh_existing_dirty_joints_filters_or_caches() {
        let mut w = PhysicsWorld::default();
        let a = w.insert_body(RigidBodyBuilder::fixed()).unwrap();
        let (b, _) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
        let joint = w
            .insert_joint(
                a,
                b,
                JointBuilder::hinge().motor(crate::JointMotor::velocity(1., 2.)),
            )
            .unwrap();
        for _ in 0..8 {
            w.step(1. / 120.);
        }
        w.joints[joint].set_contacts_enabled(true);
        assert!(w.joints[joint].dirty);
        assert!(w.joints[joint].cache.iter().any(|row| row.impulse != 0.));
        let snapshot = |w: &PhysicsWorld| {
            format!(
                "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
                w.bodies,
                w.colliders,
                w.joints,
                w.raw.disabled_joint_pairs,
                w.raw.broken_joints,
                w.raw
                    .joint_rows
                    .iter()
                    .map(|row| (row.id, row.slot, row.a, row.b, row.impulse))
                    .collect::<Vec<_>>(),
                w.last_step
            )
        };
        let mut plan = Assembly::new();
        let x = plan
            .add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0])
            .unwrap();
        let y = plan
            .add_body(RigidBodyBuilder::dynamic(), [ColliderBuilder::ball(1.)])
            .unwrap();
        plan.add_joint(x, y, JointBuilder::hinge()).unwrap();
        let invalid = plan
            .add_joint(x, y, JointBuilder::spring(1., 0., 1.))
            .unwrap();
        let before = snapshot(&w);
        assert_eq!(
            w.insert_assembly(plan),
            Err(AssemblyError::Joint {
                joint: invalid,
                error: PhysicsError::InvalidJoint,
            })
        );
        assert_eq!(snapshot(&w), before);
        assert!(w.raw.joint_solver.is_some());
    }
}
