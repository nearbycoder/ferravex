//! Bounded deferred ECS authoring, with buffer-local, epoch-local spawn tokens.
//!
//! Queue while query guards exist; apply only after they drop. Spawns use
//! [`World::try_spawn_batch`] with one owned bundle, so the caller must explicitly
//! prepare entity and component **sparse-index** budgets. Insert uses checked
//! [`World::insert`] and may grow ordinary component storage. No world identity
//! is added: existing [`Entity`] handles and resolved tokens remain world-local.
//! Apply every partial flush in an epoch to the same world; replacing the world
//! requires [`EcsCommandBuffer::reset`] first.
//!
//! Command capacity bounds pending operations; spawn capacity bounds all accepted
//! spawns in an epoch, including failed and already claimed spawns. Slots are not
//! recycled until reset. A token can target many later commands, but its spawn is
//! executed once and its output mapping can be taken once. Taking the mapping
//! does not break queued dependencies. Budgets, errors and panics preserve the
//! unvisited FIFO tail. A failed/panicking spawn never supplies a dependency.
//!
//! This adapter shares task 205's [`CommandFailurePolicy`], [`CommandFailure`]
//! and [`FallibleCommandApplyOutcome`]. Its small executor additionally borrows
//! the world and token map separately, retains typed rejected bundles and records
//! successes. The generic closure queue cannot do that directly without an owned
//! world or shared interior-mutability context. No arbitrary callback is queued.
//!
//! Storage is bounded, not universally allocation-free: construction reserves
//! the queue, mapping and success log, and creates one reference-counted identity.
//! Accepted bundle commands allocate one box; remove/despawn commands may also
//! allocate. Rejection occurs before boxing. Apply/report/map bookkeeping does
//! not allocate; ECS insert/despawn, component destructors and callbacks may.
//! Bundles need only `Bundle + 'static`, with no Clone/Debug/Send requirement.
//!
//! Reported spawn errors retain the original bundle. Dependency errors retain
//! insert bundles too. Once passed to `World::insert`, an invalid/duplicate bundle
//! is consumed by that API. Stale insert targets are rejected with the bundle
//! retained before that call. Removed components are dropped, not returned. No
//! operation, destructor, allocation or callback panic is caught or rolled back.
//! A panic can leave partial ECS mutations; inspect the world before continuing.
//!
//! # Tick-boundary recipe
//!
//! ```
//! use ferravex_engine::{ecs::{Entity, World}, ecs_commands::EcsCommandBuffer};
//! use ferravex_engine::commands::CommandFailurePolicy;
//! struct Lifetime(u32);
//! let mut world = World::new();
//! world.prepare_entities(8).unwrap();
//! world.prepare_component::<Lifetime>(8).unwrap();
//! let old = world.spawn((Lifetime(0),));
//! let mut commands = EcsCommandBuffer::new(8, 4);
//! let token;
//! {
//!     let query = world.query::<(Entity, &Lifetime)>();
//!     for (entity, lifetime) in &query {
//!         if lifetime.0 == 0 { commands.despawn(10, entity).unwrap(); }
//!     }
//!     // Author the next actor while shared world/query borrows still exist.
//!     token = commands.spawn(11, (Lifetime(3),)).unwrap_or_else(|_| panic!("full"));
//!     commands.insert(12, &token, (true,)).unwrap_or_else(|_| panic!("full"));
//! } // All ECS guards end before mutable apply.
//! let mut failures = 0;
//! let mut report = commands.apply_report(&mut world, 8, CommandFailurePolicy::Continue,
//!     |_| failures += 1).unwrap();
//! assert_eq!(failures, 0);
//! assert_eq!(report.commands.applied, 3);
//! let actor = report.take_spawn(&token).unwrap();
//! // Emit feedback only for these accepted mutations, once per returned report.
//! let feedback: Vec<_> = report.accepted().iter().map(|event| event.id).collect();
//! assert_eq!(feedback, [10, 11, 12]);
//! assert!(!world.contains(old));
//! assert!(world.contains(actor));
//! assert!(*world.get::<&bool>(actor).unwrap());
//! ```
//!
//! Rust prevents applying while a query is still used:
//!
//! ```compile_fail
//! use ferravex_engine::{ecs::World, ecs_commands::EcsCommandBuffer};
//! use ferravex_engine::commands::CommandFailurePolicy;
//! let mut world = World::new();
//! let mut commands = EcsCommandBuffer::new(1, 1);
//! let query = world.query::<&u32>();
//! commands.apply_report(&mut world, 1, CommandFailurePolicy::Continue, |_| {}).unwrap();
//! assert_eq!(query.iter().count(), 0);
//! ```
//!
//! Tokens cannot be constructed from a guessed slot or entity:
//!
//! ```compile_fail
//! use ferravex_engine::ecs_commands::SpawnToken;
//! let token = SpawnToken { epoch: 0, slot: 7 };
//! ```

use crate::commands::{CommandFailure, CommandFailurePolicy, FallibleCommandApplyOutcome};
use crate::ecs::{Bundle, Entity, Error, SpawnError, World};
use std::any::Any;
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

/// An opaque reference to one accepted spawn. Clone retains identity, not a spawn.
/// No serializable/raw constructor is supplied; this is not a world entity.
#[derive(Clone, Debug)]
pub struct SpawnToken {
    owner: Rc<()>,
    epoch: u64,
    slot: usize,
}

impl PartialEq for SpawnToken {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner)
            && self.epoch == other.epoch
            && self.slot == other.slot
    }
}
impl Eq for SpawnToken {}

/// Existing world-local identity or an earlier spawn in this buffer epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityTarget {
    Existing(Entity),
    Spawn(SpawnToken),
}
impl From<Entity> for EntityTarget {
    fn from(entity: Entity) -> Self {
        Self::Existing(entity)
    }
}
impl From<SpawnToken> for EntityTarget {
    fn from(token: SpawnToken) -> Self {
        Self::Spawn(token)
    }
}
impl From<&SpawnToken> for EntityTarget {
    fn from(token: &SpawnToken) -> Self {
        Self::Spawn(token.clone())
    }
}

/// Token provenance, dependency or one-shot mapping failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenError {
    ForeignBuffer,
    StaleEpoch,
    /// No accepted earlier spawn owns this slot (also checked defensively at apply).
    Unresolved,
    Pending,
    /// The spawn returned an error or began execution and unwound.
    DependencyFailed,
    AlreadyTaken,
    ResetIncomplete,
}

/// Authoring rejection. Rejected operations never enter the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnqueueError {
    CommandCapacity,
    SpawnCapacity,
    Token(TokenError),
    ResetIncomplete,
}

/// A rejected owned bundle, returned without cloning, boxing or dropping it.
#[derive(Debug)]
pub struct RejectedBundle<B> {
    pub reason: EnqueueError,
    pub bundle: B,
}

/// Reset cannot wrap its epoch. A panicking reset disables apply/authoring until
/// another reset finishes; no remaining old-epoch command can then be executed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferError {
    EpochExhausted,
    ResetIncomplete,
}

/// Domain failure from one consumed command. No automatic retry or rollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EcsCommandErrorKind {
    Token(TokenError),
    Spawn(SpawnError),
    Ecs(Error),
}

/// Owned failure. Use [`Self::into_bundle`] to recover a retained typed bundle.
/// A wrong requested type returns the entire error unchanged.
pub struct EcsCommandError {
    pub kind: EcsCommandErrorKind,
    payload: Option<Box<dyn Any>>,
}
impl fmt::Debug for EcsCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EcsCommandError")
            .field("kind", &self.kind)
            .field("has_bundle", &self.payload.is_some())
            .finish()
    }
}
impl EcsCommandError {
    /// Recovers a failed spawn/dependency bundle of its exact original tuple type.
    /// Stale insert targets retain bundles. Errors returned by `World::insert`
    /// itself (duplicate component types) have none: that API consumed the input.
    ///
    /// ```
    /// use ferravex_engine::{ecs::World, ecs_commands::{EcsCommandBuffer, TokenError}};
    /// use ferravex_engine::commands::CommandFailurePolicy;
    /// let mut world = World::new(); // Not yet prepared for owned String bundles.
    /// let mut commands = EcsCommandBuffer::new(2, 1);
    /// let failed = commands.spawn(20, (String::from("actor"),)).unwrap();
    /// let mut owned = None;
    /// let report = commands.apply_report(&mut world, 1, CommandFailurePolicy::StopOnError,
    ///     |failure| owned = Some(failure.error.into_bundle::<(String,)>().unwrap())).unwrap();
    /// assert_eq!(report.commands.failed, 1);
    /// assert_eq!(commands.take_spawn(&failed), Err(TokenError::DependencyFailed));
    /// commands.reset().unwrap(); // Cancels old tails and expires every old token.
    /// world.prepare_entities(1).unwrap();
    /// world.prepare_component::<String>(1).unwrap();
    /// let fresh = commands.spawn(21, owned.unwrap()).unwrap();
    /// let mut report = commands.apply_report(&mut world, 1, CommandFailurePolicy::Continue,
    ///     |_| unreachable!()).unwrap();
    /// let entity = report.take_spawn(&fresh).unwrap();
    /// assert_eq!(report.take_spawn(&fresh), Err(TokenError::AlreadyTaken));
    /// assert_eq!(&**world.get::<&String>(entity).unwrap(), "actor");
    /// assert_eq!(commands.take_spawn(&failed), Err(TokenError::StaleEpoch));
    /// commands.remove::<String>(22, &fresh).unwrap(); // Claiming kept target resolution.
    /// commands.apply_report(&mut world, 1, CommandFailurePolicy::Continue,
    ///     |_| unreachable!()).unwrap();
    /// assert!(!world.has::<String>(entity));
    /// ```
    pub fn into_bundle<B: Bundle + 'static>(mut self) -> Result<B, Self> {
        if !self
            .payload
            .as_ref()
            .is_some_and(|p| p.is::<BundleCommand<B>>())
        {
            return Err(self);
        }
        let command = self.payload.take().unwrap().downcast::<BundleCommand<B>>();
        Ok(command.ok().unwrap().bundle.take().unwrap())
    }
}

/// Kind of successfully completed mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityCommandKind {
    Spawn,
    Insert,
    Remove,
    Despawn,
}

/// Accepted mutation for host feedback/correlation. IDs are caller labels and
/// may repeat, including zero; the buffer does not deduplicate host events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedEntityCommand {
    pub id: u64,
    pub kind: EntityCommandKind,
    pub entity: Entity,
}

#[derive(Clone, Copy)]
enum SpawnState {
    Pending,
    Failed,
    Applied { entity: Entity, taken: bool },
}
struct Mapping {
    owner: Rc<()>,
    epoch: u64,
    slots: Vec<SpawnState>,
    capacity: usize,
}
impl Mapping {
    fn index(&self, token: &SpawnToken) -> Result<usize, TokenError> {
        if !Rc::ptr_eq(&self.owner, &token.owner) {
            return Err(TokenError::ForeignBuffer);
        }
        if token.epoch != self.epoch {
            return Err(TokenError::StaleEpoch);
        }
        if token.slot >= self.slots.len() {
            return Err(TokenError::Unresolved);
        }
        Ok(token.slot)
    }
    fn target(&self, target: &EntityTarget) -> Result<Entity, TokenError> {
        match target {
            EntityTarget::Existing(entity) => Ok(*entity),
            EntityTarget::Spawn(token) => match self.slots[self.index(token)?] {
                SpawnState::Pending => Err(TokenError::Pending),
                SpawnState::Failed => Err(TokenError::DependencyFailed),
                SpawnState::Applied { entity, .. } => Ok(entity),
            },
        }
    }
    fn take(&mut self, token: &SpawnToken) -> Result<Entity, TokenError> {
        let slot = self.index(token)?;
        match &mut self.slots[slot] {
            SpawnState::Pending => Err(TokenError::Pending),
            SpawnState::Failed => Err(TokenError::DependencyFailed),
            SpawnState::Applied { taken: true, .. } => Err(TokenError::AlreadyTaken),
            SpawnState::Applied { entity, taken } => {
                *taken = true;
                Ok(*entity)
            }
        }
    }
}

/// A flush's bounded accepted-event slice and the epoch's one-shot spawn map.
/// Dropping the report preserves unclaimed mappings and queued dependencies.
/// A later apply replaces the accepted-event log, including on a zero budget.
pub struct EcsApplyReport<'a> {
    pub commands: FallibleCommandApplyOutcome,
    accepted: &'a [AppliedEntityCommand],
    mapping: &'a mut Mapping,
}
impl EcsApplyReport<'_> {
    /// Successes from this flush in FIFO order, for accepted-event feedback.
    pub fn accepted(&self) -> &[AppliedEntityCommand] {
        self.accepted
    }
    /// Claims one spawn mapping; queued targets keep their private resolution.
    /// This records spawn acceptance, not current liveness after later despawns.
    pub fn take_spawn(&mut self, token: &SpawnToken) -> Result<Entity, TokenError> {
        self.mapping.take(token)
    }
}

trait Deferred {
    fn execute(
        self: Box<Self>,
        world: &mut World,
        target: Option<Entity>,
    ) -> Result<Entity, EcsCommandError>;
    fn reject(self: Box<Self>, kind: EcsCommandErrorKind) -> EcsCommandError;
}
struct BundleCommand<B> {
    bundle: Option<B>,
    spawn: bool,
}
impl<B: Bundle + 'static> Deferred for BundleCommand<B> {
    fn execute(
        mut self: Box<Self>,
        world: &mut World,
        target: Option<Entity>,
    ) -> Result<Entity, EcsCommandError> {
        if self.spawn {
            let bundle = self.bundle.take().unwrap();
            let mut output = [Entity::from_bits(0)];
            match world.try_spawn_batch([bundle], &mut output) {
                Ok(_) => Ok(output[0]),
                Err((error, [bundle])) => {
                    self.bundle = Some(bundle);
                    Err(self.reject(EcsCommandErrorKind::Spawn(error)))
                }
            }
        } else {
            let entity = target.unwrap();
            if !world.contains(entity) {
                return Err(self.reject(EcsCommandErrorKind::Ecs(Error::NoSuchEntity(entity))));
            }
            let bundle = self.bundle.take().unwrap();
            world
                .insert(entity, bundle)
                .map_err(|error| EcsCommandError {
                    kind: EcsCommandErrorKind::Ecs(error),
                    payload: None,
                })?;
            Ok(entity)
        }
    }
    fn reject(self: Box<Self>, kind: EcsCommandErrorKind) -> EcsCommandError {
        EcsCommandError {
            kind,
            payload: Some(self),
        }
    }
}
struct Remove<T>(PhantomData<fn() -> T>);
impl<T: 'static> Deferred for Remove<T> {
    fn execute(
        self: Box<Self>,
        world: &mut World,
        target: Option<Entity>,
    ) -> Result<Entity, EcsCommandError> {
        let entity = target.unwrap();
        world
            .remove::<T>(entity)
            .map(drop)
            .map_err(|error| self.reject(EcsCommandErrorKind::Ecs(error)))?;
        Ok(entity)
    }
    fn reject(self: Box<Self>, kind: EcsCommandErrorKind) -> EcsCommandError {
        EcsCommandError {
            kind,
            payload: None,
        }
    }
}
struct Despawn;
impl Deferred for Despawn {
    fn execute(
        self: Box<Self>,
        world: &mut World,
        target: Option<Entity>,
    ) -> Result<Entity, EcsCommandError> {
        let entity = target.unwrap();
        world
            .despawn(entity)
            .map_err(|error| self.reject(EcsCommandErrorKind::Ecs(error)))?;
        Ok(entity)
    }
    fn reject(self: Box<Self>, kind: EcsCommandErrorKind) -> EcsCommandError {
        EcsCommandError {
            kind,
            payload: None,
        }
    }
}
struct Queued {
    id: u64,
    kind: EntityCommandKind,
    target: Option<EntityTarget>,
    spawn_slot: Option<usize>,
    command: Box<dyn Deferred>,
}

/// Typed, bounded structural changes for one ECS world at a tick boundary.
/// See the module recipe for preparation, query lifetimes and accepted feedback.
pub struct EcsCommandBuffer {
    queue: VecDeque<Queued>,
    capacity: usize,
    mapping: Mapping,
    accepted: Vec<AppliedEntityCommand>,
    reset_incomplete: bool,
}
impl EcsCommandBuffer {
    /// Reserves bounded queue/report/map storage. Zero capacities are valid.
    /// Construction uses the ordinary allocator and can panic/abort.
    pub fn new(command_capacity: usize, spawn_capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(command_capacity),
            capacity: command_capacity,
            mapping: Mapping {
                owner: Rc::new(()),
                epoch: 0,
                slots: Vec::with_capacity(spawn_capacity),
                capacity: spawn_capacity,
            },
            accepted: Vec::with_capacity(command_capacity),
            reset_incomplete: false,
        }
    }
    /// Number of pending operations, excluding consumed errors and panics.
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    /// Pending-operation limit, independent of the epoch spawn-map limit.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.len()
    }
    /// Total accepted-spawn limit per epoch; claiming does not release slots.
    pub fn spawn_capacity(&self) -> usize {
        self.mapping.capacity
    }
    pub fn remaining_spawn_capacity(&self) -> usize {
        self.mapping.capacity - self.mapping.slots.len()
    }
    fn room(&self) -> Result<(), EnqueueError> {
        if self.reset_incomplete {
            Err(EnqueueError::ResetIncomplete)
        } else if self.len() == self.capacity {
            Err(EnqueueError::CommandCapacity)
        } else {
            Ok(())
        }
    }
    fn validate_target(&self, target: &EntityTarget) -> Result<(), EnqueueError> {
        if let EntityTarget::Spawn(token) = target {
            let index = self.mapping.index(token).map_err(EnqueueError::Token)?;
            if matches!(self.mapping.slots[index], SpawnState::Failed) {
                return Err(EnqueueError::Token(TokenError::DependencyFailed));
            }
        }
        Ok(())
    }
    /// Authors a prepared single spawn; full queue takes precedence over map full.
    /// Returns a token only after accepting the command and its mapping slot.
    pub fn spawn<B: Bundle + 'static>(
        &mut self,
        id: u64,
        bundle: B,
    ) -> Result<SpawnToken, RejectedBundle<B>> {
        let ready = self.room().and_then(|()| {
            if self.remaining_spawn_capacity() == 0 {
                Err(EnqueueError::SpawnCapacity)
            } else {
                Ok(())
            }
        });
        if let Err(reason) = ready {
            return Err(RejectedBundle { reason, bundle });
        }
        let command = Box::new(BundleCommand {
            bundle: Some(bundle),
            spawn: true,
        });
        let slot = self.mapping.slots.len();
        self.queue.push_back(Queued {
            id,
            kind: EntityCommandKind::Spawn,
            target: None,
            spawn_slot: Some(slot),
            command,
        });
        self.mapping.slots.push(SpawnState::Pending);
        Ok(SpawnToken {
            owner: Rc::clone(&self.mapping.owner),
            epoch: self.mapping.epoch,
            slot,
        })
    }
    /// Appends a checked insert. Queue capacity is checked before token validity.
    /// Ordinary entity liveness is checked at apply by the existing ECS API.
    pub fn insert<B: Bundle + 'static>(
        &mut self,
        id: u64,
        target: impl Into<EntityTarget>,
        bundle: B,
    ) -> Result<(), RejectedBundle<B>> {
        let target = target.into();
        if let Err(reason) = self.room().and_then(|()| self.validate_target(&target)) {
            return Err(RejectedBundle { reason, bundle });
        }
        self.queue.push_back(Queued {
            id,
            kind: EntityCommandKind::Insert,
            target: Some(target),
            spawn_slot: None,
            command: Box::new(BundleCommand {
                bundle: Some(bundle),
                spawn: false,
            }),
        });
        Ok(())
    }
    /// Removes and drops T at apply; absence is an ECS error, not a no-op.
    pub fn remove<T: 'static>(
        &mut self,
        id: u64,
        target: impl Into<EntityTarget>,
    ) -> Result<(), EnqueueError> {
        let target = target.into();
        self.room()?;
        self.validate_target(&target)?;
        self.queue.push_back(Queued {
            id,
            kind: EntityCommandKind::Remove,
            target: Some(target),
            spawn_slot: None,
            command: Box::new(Remove::<T>(PhantomData)),
        });
        Ok(())
    }
    /// Despawns at apply; stale entities are errors, not silently ignored.
    pub fn despawn(
        &mut self,
        id: u64,
        target: impl Into<EntityTarget>,
    ) -> Result<(), EnqueueError> {
        let target = target.into();
        self.room()?;
        self.validate_target(&target)?;
        self.queue.push_back(Queued {
            id,
            kind: EntityCommandKind::Despawn,
            target: Some(target),
            spawn_slot: None,
            command: Box::new(Despawn),
        });
        Ok(())
    }
    /// Executes at most `limit` operations; errors consume budget and commands.
    /// Stop/continue and untouched-tail behavior match the shared fallible queue.
    /// Each failure is moved synchronously to `on_error`; no error list is built.
    /// Successes are logged before any later command/callback can panic. Both
    /// command and callback panics propagate. A spawn is marked failed before
    /// invocation, then mapped only on success. No spawn is automatically retried.
    /// Only incomplete reset returns `Err` before doing work.
    pub fn apply_report(
        &mut self,
        world: &mut World,
        limit: usize,
        policy: CommandFailurePolicy,
        mut on_error: impl FnMut(CommandFailure<EcsCommandError>),
    ) -> Result<EcsApplyReport<'_>, BufferError> {
        if self.reset_incomplete {
            return Err(BufferError::ResetIncomplete);
        }
        self.accepted.clear();
        let mut outcome = FallibleCommandApplyOutcome::default();
        for _ in 0..limit.min(self.len()) {
            let queued = self.queue.pop_front().unwrap();
            outcome.started += 1;
            if let Some(slot) = queued.spawn_slot {
                self.mapping.slots[slot] = SpawnState::Failed;
            }
            let target = queued
                .target
                .as_ref()
                .map(|target| self.mapping.target(target))
                .transpose();
            let result = match target {
                Ok(target) => queued.command.execute(world, target),
                Err(error) => Err(queued.command.reject(EcsCommandErrorKind::Token(error))),
            };
            match result {
                Ok(entity) => {
                    if let Some(slot) = queued.spawn_slot {
                        self.mapping.slots[slot] = SpawnState::Applied {
                            entity,
                            taken: false,
                        };
                    }
                    outcome.applied += 1;
                    self.accepted.push(AppliedEntityCommand {
                        id: queued.id,
                        kind: queued.kind,
                        entity,
                    });
                }
                Err(error) => {
                    outcome.failed += 1;
                    on_error(CommandFailure {
                        id: queued.id,
                        error,
                    });
                    if policy == CommandFailurePolicy::StopOnError {
                        break;
                    }
                }
            }
        }
        outcome.remaining = self.len();
        Ok(EcsApplyReport {
            commands: outcome,
            accepted: &self.accepted,
            mapping: &mut self.mapping,
        })
    }
    /// Last flush's successes, also inspectable after an unwind. A later apply
    /// replaces this log. Hosts must avoid emitting the same inspected log twice.
    pub fn last_accepted(&self) -> &[AppliedEntityCommand] {
        &self.accepted
    }
    /// Claims an accepted spawn even after the report drops or a later unwind.
    /// Does not check entity liveness; a subsequent command may have despawned it.
    pub fn take_spawn(&mut self, token: &SpawnToken) -> Result<Entity, TokenError> {
        if self.reset_incomplete {
            return Err(TokenError::ResetIncomplete);
        }
        self.mapping.take(token)
    }
    /// Cancels pending work, invalidates every token and clears mappings/logs.
    /// Does not despawn accepted entities. Retains all storage and buffer identity.
    /// Epoch advancement is checked before mutation and never wraps. Tokens keep
    /// the reference-counted identity alive, so buffer drop/recreation cannot
    /// revive them. Before dropping payloads the buffer enters reset-incomplete
    /// state; if a destructor panics, the untouched tail is disabled until another
    /// reset completes. No cancellation callback runs; owned payloads are dropped.
    pub fn reset(&mut self) -> Result<(), BufferError> {
        let epoch = self
            .mapping
            .epoch
            .checked_add(1)
            .ok_or(BufferError::EpochExhausted)?;
        self.reset_incomplete = true;
        self.mapping.epoch = epoch;
        self.mapping.slots.clear();
        self.accepted.clear();
        while let Some(command) = self.queue.pop_front() {
            drop(command);
        }
        self.reset_incomplete = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn unissued_and_forward_tokens_are_rejected_before_capacity_is_spent() {
        let mut buffer = EcsCommandBuffer::new(4, 2);
        let first = buffer.spawn(1, ()).unwrap();
        let mut forward = first.clone();
        forward.slot += 1;
        for slot in [forward.slot, usize::MAX] {
            forward.slot = slot;
            assert_eq!(buffer.take_spawn(&forward), Err(TokenError::Unresolved));
            assert_eq!(
                buffer.insert(2, &forward, (7_u32,)).unwrap_err().reason,
                EnqueueError::Token(TokenError::Unresolved)
            );
            assert_eq!(
                buffer.remove::<u32>(2, &forward),
                Err(EnqueueError::Token(TokenError::Unresolved))
            );
            assert_eq!(
                buffer.despawn(2, &forward),
                Err(EnqueueError::Token(TokenError::Unresolved))
            );
        }
        assert_eq!((buffer.len(), buffer.remaining_spawn_capacity()), (1, 1));
        // Public fields/constructors cannot forge this future identity; after a
        // real second spawn, the issued token is legitimately usable.
        let second = buffer.spawn(3, ()).unwrap();
        buffer.despawn(4, &second).unwrap();
    }

    #[test]
    fn defensive_apply_rejects_pending_foreign_and_stale_dependencies() {
        for mode in 0..3 {
            let mut world = World::new();
            world.prepare_entities(1).unwrap();
            let mut buffer = EcsCommandBuffer::new(2, 1);
            let token = buffer.spawn(1, ()).unwrap();
            buffer.insert(2, &token, (9_u32,)).unwrap();
            // Deliberate private-state corruption verifies the apply boundary;
            // the public API only accepts targets of earlier issued spawns.
            buffer.queue.swap(0, 1);
            let target = buffer.queue[0].target.as_mut().unwrap();
            if let EntityTarget::Spawn(token) = target {
                match mode {
                    1 => token.owner = Rc::new(()),
                    2 => token.epoch += 1,
                    _ => {}
                }
            }
            let mut failure = None;
            let report = buffer
                .apply_report(&mut world, 2, CommandFailurePolicy::Continue, |e| {
                    failure = Some(e)
                })
                .unwrap();
            assert_eq!((report.commands.failed, report.commands.applied), (1, 1));
            let error = failure.unwrap().error;
            assert_eq!(
                error.kind,
                EcsCommandErrorKind::Token(match mode {
                    0 => TokenError::Pending,
                    1 => TokenError::ForeignBuffer,
                    _ => TokenError::StaleEpoch,
                })
            );
            assert_eq!(error.into_bundle::<(u32,)>().unwrap(), (9,));
            let entity = buffer.take_spawn(&token).unwrap();
            assert!(!world.has::<u32>(entity));
        }
    }

    #[test]
    fn epoch_exhaustion_rejects_reset_without_wrapping_or_mutating_tail() {
        let mut buffer = EcsCommandBuffer::new(2, 1);
        buffer.mapping.epoch = u64::MAX - 1;
        let old = buffer.spawn(1, ()).unwrap();
        buffer.reset().unwrap();
        let last = buffer.spawn(2, ()).unwrap();
        assert_eq!(buffer.reset(), Err(BufferError::EpochExhausted));
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.take_spawn(&old), Err(TokenError::StaleEpoch));
        assert_eq!(buffer.take_spawn(&last), Err(TokenError::Pending));
        let mut world = World::new();
        world.prepare_entities(1).unwrap();
        let mut report = buffer
            .apply_report(
                &mut world,
                1,
                CommandFailurePolicy::Continue,
                |_| unreachable!(),
            )
            .unwrap();
        assert!(report.take_spawn(&last).is_ok());
        assert_eq!(buffer.reset(), Err(BufferError::EpochExhausted));
        assert_eq!(world.len(), 1);
    }

    struct PanicSpawn;
    impl Deferred for PanicSpawn {
        fn execute(
            self: Box<Self>,
            world: &mut World,
            _: Option<Entity>,
        ) -> Result<Entity, EcsCommandError> {
            world.spawn((17_u32,));
            panic!("injected spawn execution unwind after mutation");
        }
        fn reject(self: Box<Self>, kind: EcsCommandErrorKind) -> EcsCommandError {
            EcsCommandError {
                kind,
                payload: None,
            }
        }
    }

    #[test]
    fn injected_spawn_unwind_cannot_resolve_to_partially_created_entity() {
        for policy in [
            CommandFailurePolicy::StopOnError,
            CommandFailurePolicy::Continue,
        ] {
            let mut world = World::new();
            let mut buffer = EcsCommandBuffer::new(3, 2);
            let token = buffer.spawn(1, ()).unwrap();
            // Safe sealed bundle writes have no user callback. Inject exactly the
            // executor boundary to exercise panic handling without an OOM test.
            buffer.queue[0].command = Box::new(PanicSpawn);
            buffer.despawn(2, &token).unwrap();
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = buffer.apply_report(&mut world, 2, policy, |_| unreachable!());
                }))
                .is_err()
            );
            assert_eq!(buffer.len(), 1);
            assert!(buffer.last_accepted().is_empty());
            assert_eq!(buffer.take_spawn(&token), Err(TokenError::DependencyFailed));
            assert_eq!(world.len(), 1); // Partial mutation, never rolled back.
            let report = buffer
                .apply_report(&mut world, 1, policy, |failure| {
                    assert_eq!(failure.id, 2);
                    assert_eq!(
                        failure.error.kind,
                        EcsCommandErrorKind::Token(TokenError::DependencyFailed)
                    );
                })
                .unwrap();
            assert_eq!(report.commands.failed, 1);
            assert_eq!(world.len(), 1); // No accidental despawn of the partial spawn.
        }
    }

    #[test]
    fn repeated_partial_flush_and_reset_retain_map_queue_and_log_storage() {
        let mut buffer = EcsCommandBuffer::new(4, 2);
        let map_pointer = buffer.mapping.slots.as_ptr();
        let log_pointer = buffer.accepted.as_ptr();
        let capacities = (
            buffer.queue.capacity(),
            buffer.mapping.slots.capacity(),
            buffer.accepted.capacity(),
        );
        let mut world = World::new();
        world.prepare_entities(2).unwrap();
        world.prepare_component::<u32>(2).unwrap();
        for _ in 0..64 {
            let token = buffer.spawn(1, (4_u32,)).unwrap();
            buffer.remove::<u32>(2, &token).unwrap();
            buffer.despawn(3, &token).unwrap();
            buffer
                .apply_report(
                    &mut world,
                    1,
                    CommandFailurePolicy::Continue,
                    |_| unreachable!(),
                )
                .unwrap();
            let entity = buffer.take_spawn(&token).unwrap();
            buffer
                .apply_report(
                    &mut world,
                    2,
                    CommandFailurePolicy::Continue,
                    |_| unreachable!(),
                )
                .unwrap();
            assert!(!world.contains(entity));
            buffer.reset().unwrap();
            assert_eq!(buffer.mapping.slots.as_ptr(), map_pointer);
            assert_eq!(buffer.accepted.as_ptr(), log_pointer);
            assert_eq!(
                (
                    buffer.queue.capacity(),
                    buffer.mapping.slots.capacity(),
                    buffer.accepted.capacity(),
                ),
                capacities
            );
        }
    }
}
