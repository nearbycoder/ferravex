//! Bounded mechanisms inserted only after every ordinary error is preflighted.
//!
//! Descriptors own existing dimensional bodies, colliders and local `JointDesc`s;
//! no second geometry representation or solver is involved. IDs are zero-based
//! indices in one assembly's insertion order, **not** generational world handles
//! or cross-plan tokens. A clone retains the same local indexing. Do not mix IDs
//! from unrelated plans: equal indices intentionally name the local entry.
//! Existing-world joint endpoints are excluded. Plans are consumed on insertion,
//! including on error; clone a template or call `validate_assembly` first when
//! needed. Validation is a snapshot, not a reservation against later world edits.
//!
//! A lab can author an unpowered compound rotor on a fixed support, validate it,
//! publish its visual handles only after success, and rebuild from the template
//! on reset. Checked constructors handle untrusted numeric input before staging:
//!
//! ```
//! # #[cfg(feature = "dim2")] {
//! use ferravex_physics::{Collider, assembly::Assembly, d2::{
//!     ColliderBuilder, JointBuilder, PhysicsWorld, RigidBody, RigidBodyBuilder,
//! }};
//! let mut template = Assembly::<RigidBody>::new();
//! let support = template.add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0]).unwrap();
//! let rotor = template.add_body(RigidBodyBuilder::dynamic(), [
//!     ColliderBuilder::try_cuboid(0.7, 0.1).unwrap(),
//!     ColliderBuilder::try_ball(0.2).unwrap().translation([0.5, 0.].into()),
//! ]).unwrap();
//! let hinge = template.add_joint(support, rotor, JointBuilder::hinge()).unwrap();
//! let mut world = PhysicsWorld::default();
//! world.validate_assembly(&template).unwrap();
//! let handles = world.insert_assembly(template.clone()).unwrap();
//! assert_eq!(world.bodies[handles.body(rotor).unwrap()].colliders(),
//!     handles.colliders(rotor).unwrap());
//! assert_eq!(handles.collider(rotor, 1), Some(handles.colliders(rotor).unwrap()[1]));
//! assert_eq!(world.joints[handles.joint(hinge).unwrap()].bodies(),
//!     [handles.body(support).unwrap(), handles.body(rotor).unwrap()]);
//! world.step(1. / 120.); // Insertion itself executes no simulation tick.
//! for &body in handles.bodies() { assert!(world.remove_body(body)); }
//! let reset = world.insert_assembly(template).unwrap();
//! assert_ne!(reset.body(rotor), handles.body(rotor)); // Ordinary generation reuse.
//! assert_eq!(reset.joints().len(), 1);
//! # }
//! ```
//!
//! Spatial assemblies use the same container with a different existing body type:
//!
//! ```
//! # #[cfg(feature = "dim3")] {
//! use ferravex_physics::{Collider, assembly::Assembly, d3::{
//!     ColliderBuilder, JointBuilder, PhysicsWorld, RigidBody, RigidBodyBuilder,
//! }};
//! let mut assembly = Assembly::<RigidBody>::default();
//! let anchor = assembly.add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0]).unwrap();
//! let bob = assembly.add_body(RigidBodyBuilder::dynamic()
//!     .try_translation([0., -2., 0.].into()).unwrap(),
//!     [ColliderBuilder::try_ball(0.3).unwrap()]).unwrap();
//! let spring = assembly.add_joint(anchor, bob, JointBuilder::spring(2., 20., 2.)).unwrap();
//! assert_eq!((assembly.body_count(), assembly.collider_count(), assembly.joint_count()),
//!     (2, 1, 1));
//! let mut world = PhysicsWorld::default();
//! let handles = world.insert_assembly(assembly).unwrap();
//! assert!(world.joints.get(handles.joint(spring).unwrap()).is_some());
//! world.step(1. / 120.);
//! assert!((world.bodies[handles.body(bob).unwrap()].translation().y + 2.).abs() < 1e-5);
//! # }
//! ```

use crate::{
    Collider, ColliderHandle, JointDesc, JointHandle, MAX_CHILDREN, MAX_COLLIDERS, MAX_JOINTS,
    PhysicsError, RigidBodyHandle, body::Access, world::World,
};
use std::ops::Range;

/// Maximum staged bodies, matching the existing world's 4096-body limit.
pub const MAX_ASSEMBLY_BODIES: usize = 4096;

/// Zero-based body index in an assembly. See the module recipe and lifetime rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalBodyId(usize);
impl LocalBodyId {
    /// Explicit local index, allowing forward references. Insertion checks bounds.
    pub const fn from_index(index: usize) -> Self {
        Self(index)
    }
    /// Index in assembly body order (unrelated to arena slot order).
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Zero-based joint index in an assembly. See the module recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalJointId(usize);
impl LocalJointId {
    /// Explicit local index; mapping lookups return `None` for out-of-range IDs.
    pub const fn from_index(index: usize) -> Self {
        Self(index)
    }
    /// Index in assembly joint order.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Ordinary preflight failure. No world state or arena identity is changed.
/// Forward references must resolve inside the final plan:
///
/// ```
/// # #[cfg(feature = "dim2")] {
/// use ferravex_physics::{PhysicsError, assembly::{
///     Assembly, AssemblyError, LocalBodyId, LocalJointId,
/// }, d2::{JointBuilder, PhysicsWorld, RigidBody}};
/// let mut plan = Assembly::<RigidBody>::new();
/// let future = LocalBodyId::from_index(1);
/// assert_eq!(future.index(), 1);
/// let joint = plan.add_joint(LocalBodyId::from_index(0), future,
///     JointBuilder::hinge()).unwrap();
/// assert_eq!(joint, LocalJointId::from_index(0));
/// assert_eq!(joint.index(), 0);
/// let mut world = PhysicsWorld::default();
/// assert_eq!(world.insert_assembly(plan), Err(AssemblyError::Joint {
///     joint, error: PhysicsError::InvalidHandle,
/// }));
/// assert!(world.bodies.is_empty() && world.joints.is_empty());
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AssemblyError {
    /// Combined staged/live body, collider or joint count exceeds its world cap.
    Capacity(PhysicsError),
    /// Invalid body, child geometry/settings, or dynamic mass.
    Body {
        body: LocalBodyId,
        error: PhysicsError,
    },
    /// Invalid local endpoint (`InvalidHandle`) or descriptor/type pair
    /// (`InvalidJoint`). Endpoints are checked before the descriptor.
    Joint {
        joint: LocalJointId,
        error: PhysicsError,
    },
}
impl std::fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capacity(error) => write!(f, "assembly capacity: {error}"),
            Self::Body { body, error } => write!(f, "assembly body {}: {error}", body.0),
            Self::Joint { joint, error } => write!(f, "assembly joint {}: {error}", joint.0),
        }
    }
}
impl std::error::Error for AssemblyError {}

#[derive(Clone, Debug)]
struct Body<B> {
    body: B,
    children: Vec<Collider>,
}
#[derive(Clone, Copy, Debug)]
struct Connection {
    a: LocalBodyId,
    b: LocalBodyId,
    desc: JointDesc,
}

/// Owned, bounded authoring plan; see the executable module recipes.
/// At most 4096 bodies, 16384 total colliders, 32 children per body and 4096
/// joints. Storage grows on authoring; there is no steady-tick allocation promise.
#[derive(Clone, Debug)]
pub struct Assembly<B> {
    bodies: Vec<Body<B>>,
    joints: Vec<Connection>,
    collider_count: usize,
}
impl<B> Default for Assembly<B> {
    fn default() -> Self {
        Self::new()
    }
}
impl<B> Assembly<B> {
    /// Empty plan. No storage is allocated until entries are staged.
    pub const fn new() -> Self {
        Self {
            bodies: Vec::new(),
            joints: Vec::new(),
            collider_count: 0,
        }
    }
    /// Stage an existing dimensional body and its ordered collider children.
    /// Count errors leave the plan unchanged; rejected inputs are dropped.
    /// Geometry/mass/dimension validation occurs at world preflight. Iteration
    /// stops after the first excess child, even for an unbounded iterator.
    /// Conversions and iterator/Drop panics are caller-code boundaries.
    pub fn add_body<C: Into<Collider>>(
        &mut self,
        body: impl Into<B>,
        children: impl IntoIterator<Item = C>,
    ) -> Result<LocalBodyId, PhysicsError> {
        if self.bodies.len() >= MAX_ASSEMBLY_BODIES {
            return Err(PhysicsError::BodyCapacity);
        }
        let mut staged = Vec::new();
        for child in children {
            if staged.len() >= MAX_CHILDREN {
                return Err(PhysicsError::ChildCapacity);
            }
            if self.collider_count + staged.len() >= MAX_COLLIDERS {
                return Err(PhysicsError::ColliderCapacity);
            }
            staged.push(child.into());
        }
        let body = body.into();
        let id = LocalBodyId(self.bodies.len());
        self.collider_count += staged.len();
        self.bodies.push(Body {
            body,
            children: staged,
        });
        Ok(id)
    }
    /// Stage a joint with assembly-local endpoints and ordinary local anchors.
    /// Forward references are allowed; all endpoints/descriptors are validated
    /// at insertion. `None` reference rotation captures insertion-time relative
    /// body rotation, exactly as `insert_joint` does. Capacity errors retain the
    /// plan; caller conversions and their panics occur before publication.
    pub fn add_joint(
        &mut self,
        a: LocalBodyId,
        b: LocalBodyId,
        desc: impl Into<JointDesc>,
    ) -> Result<LocalJointId, PhysicsError> {
        if self.joints.len() >= MAX_JOINTS {
            return Err(PhysicsError::JointCapacity);
        }
        let desc = desc.into();
        let id = LocalJointId(self.joints.len());
        self.joints.push(Connection { a, b, desc });
        Ok(id)
    }
    /// Staged body count; see the module recipe.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }
    /// Staged collider count across all bodies; see the module recipe.
    pub fn collider_count(&self) -> usize {
        self.collider_count
    }
    /// Staged joint count; see the module recipe.
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }
}

/// Mappings in deterministic plan order, not sorted by recycled world slot.
/// These are ordinary same-world generational handles; this result neither owns
/// nor removes world objects. After removal, its copied handles are stale. Local
/// collider IDs are `(LocalBodyId, child_index)` in the original child order.
/// See the module recipe for lookups and template-based reset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssemblyHandles {
    bodies: Vec<RigidBodyHandle>,
    colliders: Vec<ColliderHandle>,
    children: Vec<Range<usize>>,
    joints: Vec<JointHandle>,
}
impl AssemblyHandles {
    /// Resolve a plan-local body index, or `None` if out of range.
    pub fn body(&self, id: LocalBodyId) -> Option<RigidBodyHandle> {
        self.bodies.get(id.0).copied()
    }
    /// Bodies in local ID order, including collider-free anchors.
    pub fn bodies(&self) -> &[RigidBodyHandle] {
        &self.bodies
    }
    /// Resolve a body-local child index, or `None` if either index is invalid.
    pub fn collider(&self, body: LocalBodyId, child: usize) -> Option<ColliderHandle> {
        self.colliders(body)?.get(child).copied()
    }
    /// Ordered children for one body, including an empty slice for an anchor.
    pub fn colliders(&self, body: LocalBodyId) -> Option<&[ColliderHandle]> {
        self.children
            .get(body.0)
            .map(|range| &self.colliders[range.clone()])
    }
    /// Resolve a plan-local joint index, or `None` if out of range.
    pub fn joint(&self, id: LocalJointId) -> Option<JointHandle> {
        self.joints.get(id.0).copied()
    }
    /// Joints in local ID order.
    pub fn joints(&self) -> &[JointHandle] {
        &self.joints
    }
}

impl<B: Access> World<B> {
    /// Read-only, allocation-free snapshot of assembly validity and capacity.
    /// Checks combined counts first, then every body/child and dynamic mass,
    /// then every joint's local references, descriptor and endpoint kinds.
    /// No wake/cache/filter/arena or plan edits occur. Existing insertion rules
    /// apply; non-dynamic collider geometry is also checked. Use checked body
    /// builders for untrusted velocities/settings, as for ordinary insertion.
    /// A successful check reserves nothing. See the module's executable recipes.
    pub fn validate_assembly(&self, assembly: &Assembly<B>) -> Result<(), AssemblyError> {
        for (exceeded, error) in [
            (
                self.bodies.len() + assembly.bodies.len() > MAX_ASSEMBLY_BODIES,
                PhysicsError::BodyCapacity,
            ),
            (
                self.colliders.len() + assembly.collider_count > MAX_COLLIDERS,
                PhysicsError::ColliderCapacity,
            ),
            (
                self.joints.len() + assembly.joints.len() > MAX_JOINTS,
                PhysicsError::JointCapacity,
            ),
        ] {
            if exceeded {
                return Err(AssemblyError::Capacity(error));
            }
        }
        for (index, entry) in assembly.bodies.iter().enumerate() {
            self.check_assembly_compound(&entry.body, &entry.children)
                .map_err(|error| AssemblyError::Body {
                    body: LocalBodyId(index),
                    error,
                })?;
        }
        for (index, joint) in assembly.joints.iter().enumerate() {
            let id = LocalJointId(index);
            let (Some(a), Some(b)) = (
                assembly.bodies.get(joint.a.0),
                assembly.bodies.get(joint.b.0),
            ) else {
                return Err(AssemblyError::Joint {
                    joint: id,
                    error: PhysicsError::InvalidHandle,
                });
            };
            self.check_assembly_joint(&a.body, &b.body, joint.a == joint.b, joint.desc)
                .map_err(|error| AssemblyError::Joint { joint: id, error })?;
        }
        Ok(())
    }

    /// Insert the whole owned assembly after preflighting every ordinary error.
    /// Returned errors preserve all prior world state, free slots and arena
    /// generations; no insert/remove rollback consumes identities. All mapping
    /// buffers are staged before the first insertion. Success reuses unchanged
    /// compound/joint insertion semantics, including mass, waking, reference
    /// capture, collision filters and joint caches. It advances no simulation.
    ///
    /// This is **not** panic rollback or allocation-free insertion: staging,
    /// arenas, child lists and joint refresh/filter storage may allocate. OOM,
    /// allocator/iterator/conversion/Drop panics and inherited revision-exhaustion
    /// panics are outside the guarantee. A panic after publication may leave a
    /// partial mechanism and no returned mapping. Do not catch it and assume
    /// the world is unchanged. No arbitrary user callback runs after preflight.
    /// See the module recipe for a checked lab mechanism and reset handoff.
    pub fn insert_assembly(
        &mut self,
        assembly: Assembly<B>,
    ) -> Result<AssemblyHandles, AssemblyError> {
        self.validate_assembly(&assembly)?;
        let mut handles = AssemblyHandles {
            bodies: Vec::with_capacity(assembly.bodies.len()),
            colliders: Vec::with_capacity(assembly.collider_count),
            children: Vec::with_capacity(assembly.bodies.len()),
            joints: Vec::with_capacity(assembly.joints.len()),
        };
        for entry in assembly.bodies {
            // Preflight used the same validators/capacities. No user conversion
            // is repeated: both body and collider inputs are already owned.
            let body = self
                .insert_compound(entry.body, entry.children)
                .expect("assembly compound passed complete preflight");
            let start = handles.colliders.len();
            handles
                .colliders
                .extend_from_slice(&self.bodies[body].core().colliders);
            handles.children.push(start..handles.colliders.len());
            handles.bodies.push(body);
        }
        for joint in assembly.joints {
            let id = self
                .insert_joint(
                    handles.bodies[joint.a.0],
                    handles.bodies[joint.b.0],
                    joint.desc,
                )
                .expect("assembly joint passed complete preflight");
            handles.joints.push(id);
        }
        Ok(handles)
    }
}
