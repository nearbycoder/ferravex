//! Original, dependency-free entity/component storage for small Ferravex games.
//!
//! Components live in one sparse, contiguous slot array per Rust type. Entity
//! generations prevent stale handles from reaching reused slots. Iteration visits
//! live entity slots in stable order. Runtime component borrowing uses standard
//! library [`Ref`] / [`RefMut`] guards, so overlapping queries cannot create
//! aliased mutable references, even if an iterator is retained or abandoned.
//!
//! ```
//! use ferravex_ecs::{Entity, World};
//! let mut world = World::new();
//! let player = world.spawn((10_i32, 2.0_f32));
//! for (entity, mut health, speed) in &mut world.query::<(Entity, &mut i32, &f32)>() {
//!     assert_eq!(entity, player);
//!     *health += *speed as i32;
//! }
//! assert_eq!(*world.get::<&i32>(player).unwrap(), 12);
//! world.despawn(player).unwrap();
//! assert!(!world.contains(player));
//! ```
//!
//! Query results are borrow guards rather than naked references. Bind mutable
//! results with `mut`, as above. `try_query` and `try_iter` report conflicts as
//! errors; `query` and `iter` panic on invalid/overlapping borrows. Structural
//! operations require `&mut World`, so Rust prevents them while guards exist.
//!
//! ```compile_fail
//! use ferravex_ecs::World;
//! let mut world = World::new();
//! let entity = world.spawn((42_i32,));
//! let component = world.get::<&i32>(entity).unwrap();
//! world.despawn(entity).unwrap(); // A live guard borrows the world.
//! println!("{}", *component);
//! ```
#![forbid(unsafe_code)]

use std::any::{Any as StdAny, TypeId, type_name};
use std::cell::RefCell;
pub use std::cell::{Ref, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::marker::PhantomData;

/// Component storages keyed by `TypeId`. Every fetch and presence check looks
/// a storage up; `TypeId` is already a well-distributed hash, so it is used
/// directly instead of being rehashed with SipHash on each lookup.
type StorageMap = HashMap<TypeId, Box<dyn ErasedStorage>, BuildHasherDefault<TypeIdHasher>>;
#[derive(Default)]
struct TypeIdHasher(u64);
impl Hasher for TypeIdHasher {
    fn write(&mut self, bytes: &[u8]) {
        // Fallback for a `TypeId` representation that hashes as bytes.
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 ^= value;
    }
    fn write_u128(&mut self, value: u128) {
        self.0 ^= value as u64 ^ (value >> 64) as u64;
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// Opaque slot identity. A despawned entity never becomes valid again in its world.
/// Handles are world-local; two separate worlds may issue the same identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    /// Sparse slot index, useful for diagnostics. It is not a unique identity alone.
    pub fn id(self) -> u32 {
        self.index
    }
    pub fn generation(self) -> u32 {
        self.generation
    }
    /// A compact representation for local persistence. Validate with World::contains.
    pub fn to_bits(self) -> u64 {
        (u64::from(self.generation) << 32) | u64::from(self.index)
    }
    /// Reconstructs a handle; this does not prove that its entity exists.
    pub fn from_bits(bits: u64) -> Self {
        Self {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NoSuchEntity(Entity),
    MissingComponent(&'static str),
    BorrowConflict(&'static str),
    DuplicateComponent(&'static str),
    ConflictingQuery(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity(entity) => write!(f, "entity {entity:?} is not alive"),
            Self::MissingComponent(name) => write!(f, "entity has no {name} component"),
            Self::BorrowConflict(name) => {
                write!(f, "{name} component is already incompatibly borrowed")
            }
            Self::DuplicateComponent(name) => write!(f, "bundle contains {name} more than once"),
            Self::ConflictingQuery(name) => {
                write!(f, "query requests overlapping mutable access to {name}")
            }
        }
    }
}
impl std::error::Error for Error {}

/// Failure to prepare storage or preflight a bounded spawn batch.
///
/// Separate from [`Error`] to preserve that enum's exhaustive matches. A batch
/// returns this error together with all of its original owned bundles; see
/// [`World::try_spawn_batch`] for precedence and ownership rules.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpawnError {
    DuplicateComponent(&'static str),
    OutputTooSmall {
        required: usize,
        available: usize,
    },
    /// Highest selected sparse slot plus one exceeds the prepared entity budget.
    EntityCapacity {
        required: usize,
        prepared: usize,
    },
    /// A selected index exceeds this type's explicitly prepared sparse budget.
    ComponentCapacity {
        component: &'static str,
        required: usize,
        prepared: usize,
    },
    /// The request exceeds representable entity indices or usize arithmetic.
    IdentityCapacity,
    /// A standard-library try_reserve operation failed during preparation.
    AllocationFailed,
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateComponent(name) => write!(f, "bundle contains {name} more than once"),
            Self::OutputTooSmall {
                required,
                available,
            } => {
                write!(
                    f,
                    "batch needs {required} output handles, received {available}"
                )
            }
            Self::EntityCapacity { required, prepared } => {
                write!(
                    f,
                    "batch needs {required} entity slots, prepared {prepared}"
                )
            }
            Self::ComponentCapacity {
                component,
                required,
                prepared,
            } => {
                write!(
                    f,
                    "batch needs {required} {component} slots, prepared {prepared}"
                )
            }
            Self::IdentityCapacity => write!(f, "entity slot identity capacity exceeded"),
            Self::AllocationFailed => write!(f, "could not reserve spawn storage"),
        }
    }
}
impl std::error::Error for SpawnError {}

/// Failure to read an explicit selected-component duplication plan.
///
/// See [`World::plan_duplication`] for validation order and a runnable recipe.
/// Spawn admission uses the existing separate [`SpawnError`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DuplicationError {
    /// Invalid selection, stale source, missing component or conflicting borrow.
    Access(Error),
    /// Each source must occur once so the returned map has unique old identities.
    DuplicateSource(Entity),
}

impl fmt::Display for DuplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access(error) => error.fmt(f),
            Self::DuplicateSource(entity) => write!(f, "duplication repeats source {entity:?}"),
        }
    }
}
impl std::error::Error for DuplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Access(error) => Some(error),
            Self::DuplicateSource(_) => None,
        }
    }
}

/// Owned snapshot of explicitly selected components for `N` distinct sources.
///
/// Created by [`World::plan_duplication`]. This holds no world borrow and never
/// rereads sources, even after edits/despawns. Each component's `Clone` defines
/// whether its internal data stays shared (for example, an `Rc<RefCell<_>>`).
/// Dropping the plan discards the clones. Stack storage/work scales with `N` and
/// the selected bundle size; use small arrays for authoring. `Clone` may allocate.
/// The plan is not a whole-world, identity, hierarchy or reference-remapping clone.
#[derive(Debug)]
#[must_use]
pub struct DuplicationPlan<B: CloneBundle, const N: usize> {
    sources: [Entity; N],
    bundles: [B; N],
}

impl<B: CloneBundle, const N: usize> DuplicationPlan<B, N> {
    /// Spawns the captured bundles within the target's prepared sparse budgets.
    ///
    /// Returns exactly `N` `(old, new)` pairs in source order. Old identities are
    /// informational: sources need not still exist. A different target world is
    /// allowed; old/new handles then belong to their respective worlds and may
    /// have equal bits. In the same world each new handle differs from every
    /// source identity. Hosts explicitly remap any entity references afterward.
    ///
    /// Uses [`World::try_spawn_batch`] without weakening its admission guarantees:
    /// any reported error returns the entire plan for retry, without spawning,
    /// dropping or recloning components. No ECS buffers/metadata grow on success;
    /// preparation is the caller's responsibility. A valid empty plan needs none.
    /// User `Clone`/`Drop` and allocator panics/aborts are not caught or rolled back.
    /// See [`World::plan_duplication`] for a complete actor-template recipe.
    pub fn try_spawn(self, world: &mut World) -> Result<[(Entity, Entity); N], (SpawnError, Self)> {
        let Self { sources, bundles } = self;
        let mut entities = [Entity::from_bits(0); N];
        match world.try_spawn_batch(bundles, &mut entities) {
            Ok(_) => Ok(std::array::from_fn(|i| (sources[i], entities[i]))),
            Err((error, bundles)) => Err((error, Self { sources, bundles })),
        }
    }
}

#[derive(Default)]
struct Slot {
    generation: u32,
    alive: bool,
}

trait ErasedStorage {
    fn as_any(&self) -> &dyn StdAny;
    fn as_any_mut(&mut self) -> &mut dyn StdAny;
    fn erase(&mut self, index: usize);
    fn prepared_capacity(&self) -> usize;
}

struct Storage<T> {
    slots: Vec<Option<RefCell<T>>>,
    prepared: usize,
}

impl<T> Default for Storage<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            prepared: 0,
        }
    }
}

impl<T: 'static> ErasedStorage for Storage<T> {
    fn as_any(&self) -> &dyn StdAny {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn StdAny {
        self
    }
    fn erase(&mut self, index: usize) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = None;
        }
    }
    fn prepared_capacity(&self) -> usize {
        self.prepared
    }
}

/// Sparse typed storage with generational entities and checked component borrows.
///
/// RefCell deliberately makes World !Sync. Parallel mutation should use separate
/// worlds or explicit synchronization around the whole world.
#[derive(Default)]
pub struct World {
    slots: Vec<Slot>,
    free: Vec<u32>,
    storages: StorageMap,
    len: usize,
    prepared_entities: usize,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Prepares at least `capacity` total sparse entity slots and free-list entries.
    ///
    /// This is a monotonic index budget, not an additional/live entity count.
    /// Retired slots still occupy indices. It never creates entities, consumes
    /// free slots or shrinks buffers. Ordinary spawn ignores this budget and
    /// retains its existing growth behavior. Allocator rounding does not enlarge
    /// the logical budget returned by [`Self::prepared_entity_capacity`].
    ///
    /// Returns [`SpawnError::IdentityCapacity`] for unrepresentable slot counts,
    /// or [`SpawnError::AllocationFailed`] if a reservation fails. On error the
    /// logical budget, identities and contents are unchanged, but an earlier
    /// successful reservation may have enlarged a buffer. Allocator panics/aborts
    /// are not caught. See [`Self::try_spawn_batch`] for a complete wave recipe.
    ///
    /// ```
    /// use ferravex_ecs::{SpawnError, World};
    /// let mut world = World::new();
    /// world.prepare_entities(16)?;
    /// world.prepare_entities(4)?; // Never shrinks the explicit budget.
    /// assert_eq!(world.prepared_entity_capacity(), 16);
    /// assert!(world.is_empty());
    /// # Ok::<(), SpawnError>(())
    /// ```
    pub fn prepare_entities(&mut self, capacity: usize) -> Result<(), SpawnError> {
        check_identity_capacity(capacity)?;
        self.slots
            .try_reserve_exact(capacity.saturating_sub(self.slots.len()))
            .map_err(|_| SpawnError::AllocationFailed)?;
        self.free
            .try_reserve_exact(capacity.saturating_sub(self.free.len()))
            .map_err(|_| SpawnError::AllocationFailed)?;
        self.prepared_entities = self.prepared_entities.max(capacity);
        Ok(())
    }

    /// Explicit sparse entity-slot budget; initially zero, unaffected by ordinary spawn.
    /// See [`Self::prepare_entities`] for an executable example.
    pub fn prepared_entity_capacity(&self) -> usize {
        self.prepared_entities
    }

    /// Registers `T` and prepares at least `capacity` sparse component slots.
    ///
    /// Every selected entity index must fit, even when few entities contain `T`.
    /// No component values are constructed, replaced or dropped. Preparation is
    /// monotonic and independent of entity preparation. Ordinary insert/spawn may
    /// grow storage but do not increase its explicit prepared budget.
    ///
    /// Identity/reservation errors have the same meaning as [`Self::prepare_entities`].
    /// On a reported error, component values and the logical budget are unchanged;
    /// map capacity may grow. New storage is inserted only after its vector is
    /// reserved. Its Box allocation uses the ordinary allocator and may panic or
    /// abort rather than returning an error. No arbitrary allocation guarantee is made.
    ///
    /// ```
    /// use ferravex_ecs::{SpawnError, World};
    /// let mut world = World::new();
    /// world.prepare_component::<String>(16)?; // No String values are created.
    /// world.prepare_component::<String>(4)?;
    /// assert_eq!(world.prepared_component_capacity::<String>(), 16);
    /// assert_eq!(world.prepared_component_capacity::<u8>(), 0);
    /// assert!(world.is_empty());
    /// # Ok::<(), SpawnError>(())
    /// ```
    pub fn prepare_component<T: 'static>(&mut self, capacity: usize) -> Result<(), SpawnError> {
        check_identity_capacity(capacity)?;
        let id = TypeId::of::<T>();
        if let Some(storage) = self.storages.get_mut(&id) {
            return prepare_storage(
                storage
                    .as_any_mut()
                    .downcast_mut::<Storage<T>>()
                    .expect("component storage type matches its TypeId"),
                capacity,
            );
        }
        self.storages
            .try_reserve(1)
            .map_err(|_| SpawnError::AllocationFailed)?;
        let mut storage = Storage::<T>::default();
        prepare_storage(&mut storage, capacity)?;
        self.storages.insert(id, Box::new(storage));
        Ok(())
    }

    /// Explicit sparse budget for `T`; zero for unprepared/absent types.
    /// See [`Self::prepare_component`] for an executable example.
    pub fn prepared_component_capacity<T: 'static>(&self) -> usize {
        self.storages
            .get(&TypeId::of::<T>())
            .map_or(0, |storage| storage.prepared_capacity())
    }

    /// Spawns an owned fixed array in caller order, within explicitly prepared storage.
    ///
    /// Writes exactly `N` handles into the output prefix and returns `Ok(N)`.
    /// Free slots are popped in the same LIFO order as ordinary spawn, followed
    /// by new indices. Preflight inspects only those selected free slots, so an
    /// unselected high index does not require component preparation for that index.
    /// Explicit entity and component budgets must cover the highest selected
    /// index plus one. Allocation rounding and ordinary storage growth do not
    /// increase these budgets. See [`Self::prepare_entities`] and
    /// [`Self::prepare_component`].
    ///
    /// # Errors and ownership
    ///
    /// Checks, in order: duplicate component types (also for `N == 0`), output
    /// length, identity arithmetic, entity budget, then component budgets in
    /// bundle order. An empty valid batch needs no preparation and returns zero.
    /// Every reported error returns `(error, original_bundles)` without moving
    /// values out of that array, dropping components, registering storage,
    /// changing entities/free-list order, or writing any output. The caller owns
    /// the returned values and decides whether to retry or drop them. Unused
    /// output is unchanged on both success and error; its initial handles need
    /// not be valid entities.
    ///
    /// # Allocation and panic limits
    ///
    /// After successful preflight, the ECS does not grow entity, free-list, map
    /// or component buffers, allocate storage boxes or allocate metadata. Input
    /// construction happens before this call and can allocate/panic. This is an
    /// all-or-none guarantee for *reported errors*, not a panic transaction:
    /// panics/aborts are not caught or rolled back, and unconsumed owned values
    /// are dropped during unwinding. User construction/Drop is not infallible.
    /// Stack space is proportional to the caller-selected array size.
    ///
    /// A first-party wave/projectile recipe: invoke the spawn step only on an
    /// executed simulation tick, and emit feedback only after `Ok` admission.
    /// Restart despawns existing entities while retaining prepared buffers.
    ///
    /// ```
    /// use ferravex_ecs::{Entity, SpawnError, World};
    /// struct Position(f32);
    /// struct Bullet { velocity: f32, remaining: f32 }
    ///
    /// let mut world = World::new();
    /// world.prepare_entities(32)?;
    /// world.prepare_component::<Position>(32)?;
    /// world.prepare_component::<Bullet>(32)?;
    /// let mut handles = [Entity::from_bits(0); 4];
    /// let projectiles = std::array::from_fn::<_, 3, _>(|i| (
    ///     Position(i as f32), Bullet { velocity: 8.0, remaining: 1.0 },
    /// ));
    /// let accepted = world.try_spawn_batch(projectiles, &mut handles)
    ///     .map_err(|(error, _owned_projectiles)| error)?;
    /// assert_eq!(accepted, 3); // Now emit three accepted-shot feedback events.
    /// let fixed_dt = 0.25;
    /// for (mut position, mut bullet) in world.query::<(&mut Position, &mut Bullet)>() {
    ///     position.0 += bullet.velocity * fixed_dt;
    ///     bullet.remaining -= fixed_dt;
    /// }
    /// assert_eq!(world.get::<&Position>(handles[0]).unwrap().0, 2.0);
    /// assert_eq!(world.get::<&Bullet>(handles[0]).unwrap().remaining, 0.75);
    /// let old = handles[0];
    /// world.despawn_many(&handles[..accepted]).unwrap();
    /// world.try_spawn_batch([(Position(10.0), Bullet { velocity: 4.0, remaining: 2.0 })],
    ///     &mut handles).map_err(|(error, _)| error)?;
    /// assert!(!world.contains(old));
    /// assert_eq!(world.len(), 1);
    /// # Ok::<(), SpawnError>(())
    /// ```
    pub fn try_spawn_batch<B: Bundle, const N: usize>(
        &mut self,
        bundles: [B; N],
        output: &mut [Entity],
    ) -> Result<usize, (SpawnError, [B; N])> {
        if let Err(error) = self.preflight_batch::<B>(N, output.len()) {
            return Err((error, bundles));
        }
        for (bundle, handle) in bundles.into_iter().zip(output.iter_mut()) {
            *handle = self.spawn_validated(bundle);
        }
        Ok(N)
    }

    /// Captures selected `Clone` component types for a small set of source entities.
    ///
    /// `B` is an owned tuple of zero to eight explicitly chosen component types.
    /// Nothing else is copied, including transient handles, unless selected.
    /// Entity identity is never copied automatically; an explicitly stored Entity
    /// component is just a value. Embedded entity references retain their old
    /// values until the host explicitly remaps them using the returned spawn map.
    /// No reflection, serialization or hierarchy traversal is performed.
    ///
    /// Checks duplicate component types (even for `N == 0`), all source identities
    /// in order, then repeated sources, then reads in source/tuple order. Every
    /// selected component is shared-borrowed before the first `Clone` call. Missing
    /// components and outstanding mutable borrows are errors, never omissions.
    /// Any reported error releases partial guards, calls no component `Clone`,
    /// changes no ECS storage/identities and spawns nothing. Shared guards and
    /// mutable guards of unselected components do not prevent capture.
    ///
    /// The returned plan owns a snapshot and releases all read guards. It can
    /// outlive source edits/despawns. No ECS metadata heap allocation is needed;
    /// fixed-array stack space scales with `N` and `B`. User `Clone` can allocate,
    /// mutate shared interior state or panic; no rollback of user effects or
    /// promise about panicking `Clone`/`Drop` is made. Successful earlier clones
    /// are dropped on unwind. Spawn capacity is checked later by the plan.
    /// Handles remain world-local; a foreign matching identity cannot be detected.
    ///
    /// This actor-template recipe omits a transient audio handle, explicitly
    /// remaps a partner reference, and despawns the duplicates for reset. Run
    /// authoring at a structural-edit boundary after query guards are released;
    /// derive gameplay feedback only from accepted spawns.
    ///
    /// ```
    /// use ferravex_ecs::World;
    /// #[derive(Clone)] struct Position(f32);
    /// #[derive(Clone)] struct Health(u32);
    /// #[derive(Clone)] struct Partner(ferravex_ecs::Entity);
    /// struct AudioVoice; // Transient, deliberately not Clone or selected.
    /// let mut world = World::new();
    /// let first = world.spawn((Position(1.0), Health(10), AudioVoice));
    /// let second = world.spawn((Position(3.0), Health(20), AudioVoice));
    /// world.insert(first, (Partner(second),))?;
    /// world.insert(second, (Partner(first),))?;
    /// world.prepare_entities(4)?;
    /// world.prepare_component::<Position>(4)?;
    /// world.prepare_component::<Health>(4)?;
    /// world.prepare_component::<Partner>(4)?;
    /// let plan = world.plan_duplication::<(Position, Health, Partner), 2>([first, second])?;
    /// let map = plan.try_spawn(&mut world).map_err(|(error, _plan)| error)?;
    /// assert_eq!(map.map(|(old, _)| old), [first, second]);
    /// let copies = map.map(|(_, new)| new);
    /// for (old, new) in map {
    ///     assert_ne!(old, new);
    ///     assert!(!world.has::<AudioVoice>(new));
    ///     let mut partner = world.get::<&mut Partner>(new)?;
    ///     let replacement = map.iter().find(|(source, _)| *source == partner.0).unwrap().1;
    ///     partner.0 = replacement; // Explicit host remap, after all copies exist.
    /// }
    /// world.get::<&mut Position>(copies[0])?.0 += 5.0;
    /// assert_eq!(world.get::<&Position>(first)?.0, 1.0);
    /// assert_eq!(world.get::<&Position>(copies[0])?.0, 6.0);
    /// assert_eq!(world.get::<&Health>(copies[1])?.0, 20);
    /// assert_eq!(world.get::<&Partner>(copies[0])?.0, copies[1]);
    /// assert_eq!(world.get::<&Partner>(first)?.0, second);
    /// world.despawn_many(&copies)?; // Prepared buffers survive reset.
    /// let again = world.plan_duplication::<(Position, Health), 1>([first])?
    ///     .try_spawn(&mut world).map_err(|(error, _)| error)?;
    /// assert!(copies.iter().all(|old| !world.contains(*old)));
    /// assert_eq!(world.get::<&Health>(again[0].1)?.0, 10);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn plan_duplication<B: CloneBundle, const N: usize>(
        &self,
        sources: [Entity; N],
    ) -> Result<DuplicationPlan<B, N>, DuplicationError> {
        validate_bundle::<B>().map_err(DuplicationError::Access)?;
        for entity in sources {
            self.validate(entity).map_err(DuplicationError::Access)?;
        }
        for (i, &entity) in sources.iter().enumerate() {
            if sources[..i].contains(&entity) {
                return Err(DuplicationError::DuplicateSource(entity));
            }
        }
        let mut borrowed = std::array::from_fn::<_, N, _>(|_| None);
        for (item, entity) in borrowed.iter_mut().zip(sources) {
            *item = Some(B::borrow(self, entity).map_err(DuplicationError::Access)?);
        }
        let bundles = borrowed.each_ref().map(|item| {
            B::clone_components(item.as_ref().expect("every source was fully borrowed"))
        });
        Ok(DuplicationPlan { sources, bundles })
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.slots
            .get(entity.index as usize)
            .is_some_and(|slot| slot.alive && slot.generation == entity.generation)
    }

    /// Spawns one entity containing zero to eight components. See [`Bundle`]
    /// for a complete five-component actor recipe.
    /// Panics if a bundle repeats a component type; no entity is allocated in that case.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        self.try_spawn(bundle)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_spawn<B: Bundle>(&mut self, bundle: B) -> Result<Entity, Error> {
        validate_bundle::<B>()?;
        Ok(self.spawn_validated(bundle))
    }

    fn spawn_validated<B: Bundle>(&mut self, bundle: B) -> Entity {
        let index = if let Some(index) = self.free.pop() {
            index
        } else {
            let index = u32::try_from(self.slots.len()).expect("entity slot capacity exceeded");
            self.slots.push(Slot::default());
            index
        };
        let slot = &mut self.slots[index as usize];
        slot.alive = true;
        let entity = Entity {
            index,
            generation: slot.generation,
        };
        self.len += 1;
        bundle.write(self, index as usize);
        entity
    }

    /// Drops every component and invalidates the handle before reusing its slot.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), Error> {
        self.validate(entity)?;
        let slot = &mut self.slots[entity.index as usize];
        slot.alive = false;
        self.len -= 1;
        // Never wrap a generation: a fully exhausted slot is permanently retired.
        let next_generation = slot.generation.checked_add(1);
        if let Some(next) = next_generation {
            slot.generation = next;
        }
        for storage in self.storages.values_mut() {
            storage.erase(entity.index as usize);
        }
        // A panicking component destructor leaves the slot retired, never recycled
        // with leftover components from the despawned entity.
        if next_generation.is_some() {
            self.free.push(entity.index);
        }
        Ok(())
    }

    /// Despawns a batch after validating every handle, returning the number removed.
    ///
    /// Repeated live handles are accepted and removed exactly once, in order of
    /// first occurrence. An empty slice returns `Ok(0)`. Successful removal has
    /// the same component cleanup and slot-reuse behavior as [`World::despawn`].
    /// The order in which different component types are dropped is unspecified.
    ///
    /// Validation and duplicate handling use constant extra space, perform no
    /// allocation and do not scan unrelated entities. Removal may grow the
    /// world's existing free-slot list, just like individual despawns.
    ///
    /// # Errors
    ///
    /// The first stale or invalid handle in input order returns
    /// [`Error::NoSuchEntity`] before any mutation or component drops. Every
    /// handle must be live at entry, including repeats. On `Err`, all entities,
    /// components, generations and slot-reuse order remain unchanged. Handles
    /// are world-local; matching identities from another world cannot be detected.
    ///
    /// # Panics
    ///
    /// Component destructors may panic. Such a panic is not rolled back: earlier
    /// removals remain applied, the current entity is invalidated and its slot
    /// retired, and unprocessed entities remain live, as with sequential despawns.
    ///
    /// ```
    /// use ferravex_ecs::{Error, World};
    ///
    /// let mut world = World::new();
    /// let first = world.spawn((10_i32,));
    /// let second = world.spawn((20_i32,));
    /// let stale = world.spawn(());
    /// world.despawn(stale)?;
    /// assert_eq!(world.despawn_many(&[first, stale]), Err(Error::NoSuchEntity(stale)));
    /// assert_eq!(*world.get::<&i32>(first)?, 10);
    /// assert_eq!(world.despawn_many(&[second, first, second])?, 2);
    /// assert!(world.is_empty());
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// Structural removal requires exclusive access, including when guards came
    /// from [`World::get_many`]:
    ///
    /// ```compile_fail
    /// use ferravex_ecs::World;
    /// let mut world = World::new();
    /// let entity = world.spawn((42_i32,));
    /// let guards = world.get_many::<&mut i32, 1>([entity]).unwrap();
    /// world.despawn_many(&[entity]).unwrap();
    /// drop(guards);
    /// ```
    pub fn despawn_many(&mut self, entities: &[Entity]) -> Result<usize, Error> {
        for &entity in entities {
            self.validate(entity)?;
        }
        let mut removed = 0;
        for &entity in entities {
            // Exclusive access keeps validation valid. A dead handle here can
            // only be a duplicate already removed earlier in this batch.
            if self.contains(entity) {
                self.despawn(entity)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Inserts or replaces a bundle's components. Other components remain unchanged.
    /// Invalid identities and duplicate bundle types are rejected before any writes.
    pub fn insert<B: Bundle>(&mut self, entity: Entity, bundle: B) -> Result<(), Error> {
        self.validate(entity)?;
        validate_bundle::<B>()?;
        bundle.write(self, entity.index as usize);
        Ok(())
    }

    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Result<T, Error> {
        self.validate(entity)?;
        self.storages
            .get_mut(&TypeId::of::<T>())
            .and_then(|s| s.as_any_mut().downcast_mut::<Storage<T>>())
            .and_then(|storage| storage.slots.get_mut(entity.index as usize))
            .and_then(Option::take)
            .map(RefCell::into_inner)
            .ok_or(Error::MissingComponent(type_name::<T>()))
    }

    pub fn has<T: 'static>(&self, entity: Entity) -> bool {
        self.contains(entity) && self.cell::<T>(entity.index as usize).is_some()
    }

    /// Reads or mutates a typed component through an explicit access type.
    /// Use `get::<&T>` or `get::<&mut T>`; returned guards enforce runtime aliasing.
    /// `Option<&T>` and `Option<&mut T>` return `None` for an absent component.
    /// Invalid entities and conflicting borrows still return errors.
    ///
    /// ```
    /// use ferravex_ecs::{Error, World};
    /// let mut world = World::new();
    /// let entity = world.spawn(());
    /// assert!(world.get::<Option<&i32>>(entity)?.is_none());
    /// world.insert(entity, (10_i32,))?;
    /// let mut health = world.get::<Option<&mut i32>>(entity)?.unwrap();
    /// *health += 1;
    /// assert!(matches!(world.get::<Option<&i32>>(entity), Err(Error::BorrowConflict(_))));
    /// drop(health);
    /// assert_eq!(*world.get::<Option<&i32>>(entity)?.unwrap(), 11);
    /// # Ok::<(), Error>(())
    /// ```
    pub fn get<A: Access>(&self, entity: Entity) -> Result<A::Item<'_>, Error> {
        self.validate(entity)?;
        A::fetch(self, entity)
    }

    pub fn get_mut<T: 'static>(&self, entity: Entity) -> Result<RefMut<'_, T>, Error> {
        self.get::<&mut T>(entity)
    }

    /// Borrows a fixed set of entities in the supplied order, without scanning the world.
    ///
    /// `Q` accepts the same single access or tuple of up to eight accesses as
    /// [`World::query`]. Returned guards borrow the world, not the input array.
    /// This performs no heap allocation and uses stack space proportional to `N`.
    /// An empty array succeeds if the query signature is valid.
    ///
    /// # Errors
    ///
    /// An overlapping mutable access within `Q` returns [`Error::ConflictingQuery`].
    /// Next, all handles are checked in input order: a despawned, reused or invalid
    /// handle returns [`Error::NoSuchEntity`] before any components are borrowed.
    /// Handles remain world-local; identities from another world cannot be detected.
    ///
    /// Components are then borrowed in entity and tuple order. A missing required
    /// component returns [`Error::MissingComponent`]; a missing optional component
    /// yields `None`. Incompatible outstanding guards or a repeated entity with
    /// mutable access to a present component return [`Error::BorrowConflict`].
    /// Repeated entities with absent optional mutable components are allowed.
    /// Repeated entities are allowed for shared access and identity-only queries.
    /// Any error releases every guard acquired by this call, including partial
    /// tuple borrows. No components are changed and no partial result is returned.
    ///
    /// ```
    /// use ferravex_ecs::{Error, World};
    ///
    /// let mut world = World::new();
    /// let donor = world.spawn((10_i32,));
    /// let receiver = world.spawn((2_i32,));
    /// {
    ///     let [mut from, mut to] = world.get_many::<&mut i32, 2>([donor, receiver])?;
    ///     *from -= 3;
    ///     *to += 3;
    /// }
    /// assert_eq!(*world.get::<&i32>(receiver)?, 5);
    /// assert!(matches!(
    ///     world.get_many::<&mut i32, 2>([donor, donor]),
    ///     Err(Error::BorrowConflict(_))
    /// ));
    ///
    /// // A tuple query can also retrieve several components of one known entity.
    /// let [(id, health)] = world.get_many::<(ferravex_ecs::Entity, &i32), 1>([donor])?;
    /// assert_eq!(id, donor);
    /// assert_eq!(*health, 7);
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// Optional accesses preserve the requested order and borrow only components
    /// that are present:
    ///
    /// ```
    /// use ferravex_ecs::{Entity, Error, World};
    /// let mut world = World::new();
    /// let actor = world.spawn((10_i32,));
    /// let scenery = world.spawn(());
    /// let [(scenery_id, absent), (actor_id, health)] =
    ///     world.get_many::<(Entity, Option<&mut i32>), 2>([scenery, actor])?;
    /// assert_eq!((scenery_id, actor_id), (scenery, actor));
    /// assert!(absent.is_none());
    /// *health.unwrap() += 1;
    /// assert_eq!(*world.get::<&i32>(actor)?, 11);
    /// # Ok::<(), Error>(())
    /// ```
    pub fn get_many<Q: Query, const N: usize>(
        &self,
        entities: [Entity; N],
    ) -> Result<[Q::Item<'_>; N], Error> {
        Q::validate()?;
        for entity in entities {
            self.validate(entity)?;
        }
        // Options let partially initialized results drop normally on failure,
        // without unsafe code or a temporary heap allocation.
        let mut items = std::array::from_fn::<_, N, _>(|_| None);
        for (item, entity) in items.iter_mut().zip(entities) {
            *item = Some(Q::fetch(self, entity)?);
        }
        Ok(items.map(|item| item.expect("every requested entity was borrowed")))
    }

    /// Queries up to eight access types, counting each [`Entity`] entry.
    /// Repeated shared access is valid; any repeated mutable access is rejected.
    ///
    /// `Option<&T>` yields `Option<Ref<'_, T>>`; `Option<&mut T>` yields
    /// `Option<RefMut<'_, T>>`. Only required accesses filter entities: an
    /// optional-only query visits every live entity, including componentless ones.
    /// A missing optional component yields `None`, but an incompatible borrow of
    /// a present component still fails. Use [`QueryBorrow::try_iter`] for fallible
    /// iteration; ordinary iteration panics on borrow conflicts.
    ///
    /// Optional access participates in signature validation even if the component
    /// is absent everywhere. For example, `(&T, Option<&mut T>)` is rejected by
    /// [`World::try_query`] with [`Error::ConflictingQuery`].
    ///
    /// This fixed-step recipe updates moving actors while retaining stationary
    /// scenery and optionally recording presentation changes:
    ///
    /// ```
    /// use ferravex_ecs::{Entity, Error, World};
    /// struct Position(f32);
    /// struct Velocity(f32);
    /// struct Presentation { updates: u32 }
    ///
    /// let mut world = World::new();
    /// let actor = world.spawn((Position(1.0), Velocity(2.0), Presentation { updates: 0 }));
    /// let scenery = world.spawn((Position(5.0),));
    /// world.spawn((Velocity(99.0),)); // No Position: excluded by the required access.
    /// let mut visited = 0;
    /// let fixed_dt = 0.5;
    /// for row in world.query::<(Entity, &mut Position, Option<&Velocity>, Option<&mut Presentation>)>().try_iter() {
    ///     let (entity, mut position, velocity, presentation) = row?;
    ///     if let Some(velocity) = velocity {
    ///         position.0 += velocity.0 * fixed_dt;
    ///         if let Some(mut presentation) = presentation {
    ///             presentation.updates += 1;
    ///         }
    ///     }
    ///     assert!(entity == actor || entity == scenery);
    ///     visited += 1;
    /// }
    /// assert_eq!(visited, 2);
    /// assert_eq!(world.get::<&Position>(actor)?.0, 2.0);
    /// assert_eq!(world.get::<&Position>(scenery)?.0, 5.0);
    /// assert_eq!(world.get::<&Presentation>(actor)?.updates, 1);
    /// # Ok::<(), Error>(())
    /// ```
    pub fn query<Q: Query>(&self) -> QueryBorrow<'_, Q> {
        self.try_query().unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_query<Q: Query>(&self) -> Result<QueryBorrow<'_, Q>, Error> {
        self.try_query_filtered::<Q, ()>()
    }

    /// Queries fetched accesses `Q` subject to structural component filter `F`.
    ///
    /// Filters inspect presence only: they never borrow component values or add
    /// accesses to `Q`'s alias checks. Both the filter and all required accesses
    /// must match before any value is borrowed. [`With`], [`Without`], [`All`],
    /// [`Any`] and [`Not`] compose through tuples of up to four clauses per group.
    /// `()` accepts every live entity; `Any<()>` accepts none. Nest groups for
    /// larger expressions. There is no runtime filter storage or allocation;
    /// expression size and nesting are fixed by the caller's concrete type.
    ///
    /// Live slots retain their normal order. Invalid fetched signatures panic;
    /// use [`World::try_query_filtered`] to report them instead. As with
    /// [`World::query`], [`QueryBorrow::try_iter`] reports fetched borrow conflicts
    /// and continues, while ordinary iteration panics on them. Contradictory
    /// filters simply match nothing and never excuse an invalid `Q` signature.
    ///
    /// This fixed-tick recipe moves active actors or projectiles, excludes frozen
    /// entities, and optionally records presentation updates. Holding a mutable
    /// marker guard does not interfere with filtering:
    ///
    /// ```
    /// use ferravex_ecs::{All, Any, Entity, Error, Not, With, Without, World};
    /// struct Position(f32);
    /// struct Velocity(f32);
    /// struct Presentation(u32);
    /// struct Active;
    /// struct Projectile;
    /// struct Frozen;
    ///
    /// let mut world = World::new();
    /// let actor = world.spawn((Position(1.0), Velocity(2.0), Active, Presentation(0)));
    /// let projectile = world.spawn((Position(4.0), Projectile));
    /// let frozen = world.spawn((Position(9.0), Active, Frozen));
    /// world.spawn((Position(20.0),)); // Neither Active nor Projectile.
    /// let marker = world.get::<&mut Active>(actor)?;
    /// type Moving = All<(Any<(With<Active>, With<Projectile>)>, Without<Frozen>)>;
    /// let fixed_dt = 0.5; // Call once per executed simulation tick.
    /// let mut visited = 0;
    /// for row in world.query_filtered::<
    ///     (Entity, &mut Position, Option<&Velocity>, Option<&mut Presentation>), Moving
    /// >().try_iter() {
    ///     let (entity, mut position, velocity, presentation) = row?;
    ///     if let Some(velocity) = velocity { position.0 += velocity.0 * fixed_dt; }
    ///     if let Some(mut presentation) = presentation { presentation.0 += 1; }
    ///     assert!(entity == actor || entity == projectile);
    ///     visited += 1;
    /// }
    /// assert_eq!(visited, 2);
    /// assert_eq!(world.get::<&Position>(actor)?.0, 2.0);
    /// assert_eq!(world.get::<&Presentation>(actor)?.0, 1);
    /// assert_eq!(world.get::<&Position>(frozen)?.0, 9.0);
    /// assert_eq!(world.query_filtered::<Entity, Not<Moving>>().iter().count(), 2);
    /// drop(marker);
    /// # Ok::<(), Error>(())
    /// ```
    pub fn query_filtered<Q: Query, F: QueryFilter>(&self) -> QueryBorrow<'_, Q, F> {
        self.try_query_filtered()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Validates fetched access aliases before returning a filtered query.
    ///
    /// Presence filters introduce no borrow conflicts. Only `Q` is validated;
    /// dynamic borrow errors arise when iterating matching entities. Validation
    /// still runs on an empty world or a filter that excludes every entity.
    ///
    /// ```
    /// use ferravex_ecs::{Any, Error, With, World};
    /// let world = World::new();
    /// assert!(matches!(
    ///     world.try_query_filtered::<(&mut i32, Option<&i32>), Any<()>>(),
    ///     Err(Error::ConflictingQuery(_))
    /// ));
    /// assert_eq!(world.try_query_filtered::<&mut i32, With<i32>>()?.iter().count(), 0);
    /// # Ok::<(), Error>(())
    /// ```
    pub fn try_query_filtered<Q: Query, F: QueryFilter>(
        &self,
    ) -> Result<QueryBorrow<'_, Q, F>, Error> {
        Q::validate()?;
        Ok(QueryBorrow {
            world: self,
            marker: PhantomData,
        })
    }

    /// Live identities in stable slot order, without borrowing any components.
    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.alive.then_some(Entity {
                index: index as u32,
                generation: slot.generation,
            })
        })
    }

    fn validate(&self, entity: Entity) -> Result<(), Error> {
        if self.contains(entity) {
            Ok(())
        } else {
            Err(Error::NoSuchEntity(entity))
        }
    }

    fn preflight_batch<B: Bundle>(&self, count: usize, output: usize) -> Result<(), SpawnError> {
        validate_bundle::<B>().map_err(|error| match error {
            Error::DuplicateComponent(name) => SpawnError::DuplicateComponent(name),
            _ => unreachable!("bundle validation only checks duplicate types"),
        })?;
        if output < count {
            return Err(SpawnError::OutputTooSmall {
                required: count,
                available: output,
            });
        }
        let required = required_batch_slots(self.slots.len(), &self.free, count)?;
        if required > self.prepared_entities {
            return Err(SpawnError::EntityCapacity {
                required,
                prepared: self.prepared_entities,
            });
        }
        for (id, component) in B::component_types_inline().into_iter().flatten() {
            let prepared = self
                .storages
                .get(&id)
                .map_or(0, |storage| storage.prepared_capacity());
            if required > prepared {
                return Err(SpawnError::ComponentCapacity {
                    component,
                    required,
                    prepared,
                });
            }
        }
        Ok(())
    }

    /// One shared, out-of-line lookup for every component type's fetches.
    #[inline(never)]
    fn erased_storage(&self, id: TypeId) -> Option<&dyn ErasedStorage> {
        self.storages.get(&id).map(|storage| &**storage)
    }

    fn cell<T: 'static>(&self, index: usize) -> Option<&RefCell<T>> {
        self.erased_storage(TypeId::of::<T>())
            .and_then(|s| s.as_any().downcast_ref::<Storage<T>>())
            .and_then(|storage| storage.slots.get(index))
            .and_then(Option::as_ref)
    }

    fn put<T: 'static>(&mut self, index: usize, value: T) {
        let storage = self
            .storages
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::<Storage<T>>::default())
            .as_any_mut()
            .downcast_mut::<Storage<T>>()
            .expect("component storage type matches its TypeId");
        if storage.slots.len() <= index {
            storage.slots.resize_with(index + 1, || None);
        }
        storage.slots[index] = Some(RefCell::new(value));
    }
}

fn check_identity_capacity(capacity: usize) -> Result<(), SpawnError> {
    if capacity > 0 && u32::try_from(capacity - 1).is_err() {
        Err(SpawnError::IdentityCapacity)
    } else {
        Ok(())
    }
}

fn required_batch_slots(slots: usize, free: &[u32], count: usize) -> Result<usize, SpawnError> {
    if count == 0 {
        return Ok(0);
    }
    let new = count.saturating_sub(free.len());
    let required = if new > 0 {
        slots.checked_add(new).ok_or(SpawnError::IdentityCapacity)?
    } else {
        // Only selected free slots matter, not the whole sparse high-water mark.
        (free.iter().rev().take(count).copied().max().unwrap() as usize)
            .checked_add(1)
            .ok_or(SpawnError::IdentityCapacity)?
    };
    check_identity_capacity(required)?;
    Ok(required)
}

fn prepare_storage<T>(storage: &mut Storage<T>, capacity: usize) -> Result<(), SpawnError> {
    storage
        .slots
        .try_reserve_exact(capacity.saturating_sub(storage.slots.len()))
        .map_err(|_| SpawnError::AllocationFailed)?;
    if capacity > storage.slots.len() {
        storage.slots.resize_with(capacity, || None);
    }
    storage.prepared = storage.prepared.max(capacity);
    Ok(())
}

mod sealed {
    pub trait Bundle {
        fn component_types_inline() -> [Option<(std::any::TypeId, &'static str)>; 8];
    }
    pub trait Access {}
    pub trait CloneBundle {}
    pub trait Query {}
    pub trait QueryFilter {}
}

/// Tuple of zero to eight owned component values accepted by spawn and insert.
/// Implementations are sealed so validation and insertion cannot disagree.
/// Duplicate types are rejected before any entity allocation or component write.
/// Validation uses bounded stack metadata; entity and component storage may allocate.
///
/// This actor recipe authors five components together and updates them once per
/// executed simulation tick, using six fetched entries including the identity:
///
/// ```
/// use ferravex_ecs::{Entity, Error, With, World};
/// struct Position(f32);
/// struct Velocity(f32);
/// struct Health(u32);
/// struct Faction(u8);
/// struct Presentation { last_position: f32 }
///
/// let mut world = World::new();
/// let actor = world.try_spawn((
///     Position(1.0), Velocity(2.0), Health(10), Faction(1),
///     Presentation { last_position: 1.0 },
/// ))?;
/// let fixed_dt = 0.5; // Caller invokes this system once per executed tick.
/// for row in world.query_filtered::<(
///     Entity, &mut Position, &Velocity, &Health, &Faction, Option<&mut Presentation>,
/// ), With<Faction>>().try_iter() {
///     let (id, mut position, velocity, health, faction, presentation) = row?;
///     assert_eq!(id, actor);
///     if health.0 > 0 && faction.0 == 1 {
///         position.0 += velocity.0 * fixed_dt;
///         if let Some(mut presentation) = presentation {
///             presentation.last_position = position.0;
///         }
///     }
/// }
/// assert_eq!(world.get::<&Position>(actor)?.0, 2.0);
/// assert_eq!(world.get::<&Presentation>(actor)?.last_position, 2.0);
/// world.despawn(actor)?;
/// assert!(world.is_empty());
/// # Ok::<(), Error>(())
/// ```
///
/// Nine component entries exceed the ceiling, even if their types are distinct:
///
/// ```compile_fail
/// use ferravex_ecs::World;
/// World::new().spawn((0_u8, 0_u16, 0_u32, 0_u64, 0_i8, 0_i16, 0_i32, 0_i64, false));
/// ```
pub trait Bundle: sealed::Bundle {
    #[doc(hidden)]
    fn component_types(types: &mut Vec<(TypeId, &'static str)>);
    #[doc(hidden)]
    fn write(self, world: &mut World, index: usize);
}

/// Explicit tuple of zero to eight owned `Clone + 'static` component types.
///
/// Sealed to keep selected reads and spawned bundles consistent. See
/// [`World::plan_duplication`] for a complete actor-template recipe. Choose stored
/// component types rather than query access types, for example `(Position, Health)`.
/// An `Entity` entry selects an explicitly stored Entity component, not identity.
///
/// Non-Clone components remain supported by ordinary bundles, but cannot be
/// selected for duplication:
///
/// ```compile_fail
/// use ferravex_ecs::World;
/// struct AudioVoice;
/// let mut world = World::new();
/// let entity = world.spawn((AudioVoice,));
/// world.plan_duplication::<(AudioVoice,), 1>([entity]);
/// ```
pub trait CloneBundle: Bundle + sealed::CloneBundle {
    #[doc(hidden)]
    type Borrowed<'world>;
    #[doc(hidden)]
    fn borrow(world: &World, entity: Entity) -> Result<Self::Borrowed<'_>, Error>;
    #[doc(hidden)]
    fn clone_components(components: &Self::Borrowed<'_>) -> Self;
}

impl sealed::Bundle for () {
    fn component_types_inline() -> [Option<(TypeId, &'static str)>; 8] {
        [None; 8]
    }
}
impl Bundle for () {
    fn component_types(_: &mut Vec<(TypeId, &'static str)>) {}
    fn write(self, _: &mut World, _: usize) {}
}
impl sealed::CloneBundle for () {}
impl CloneBundle for () {
    type Borrowed<'world> = ();
    fn borrow(_: &World, _: Entity) -> Result<(), Error> {
        Ok(())
    }
    fn clone_components(_: &()) -> Self {}
}

macro_rules! bundle {
    ($($name:ident : $index:tt),+) => {
        impl<$($name: 'static),+> sealed::Bundle for ($($name,)+) {
            fn component_types_inline() -> [Option<(TypeId, &'static str)>; 8] {
                let mut types = [None; 8];
                $(types[$index] = Some((TypeId::of::<$name>(), type_name::<$name>()));)+
                types
            }
        }
        impl<$($name: 'static),+> Bundle for ($($name,)+) {
            fn component_types(types: &mut Vec<(TypeId, &'static str)>) {
                // Preserve the existing public metadata API for callers that use it.
                types.extend(<Self as sealed::Bundle>::component_types_inline().into_iter().flatten());
            }
            fn write(self, world: &mut World, index: usize) {
                $(world.put(index, self.$index);)+
            }
        }
        impl<$($name: Clone + 'static),+> sealed::CloneBundle for ($($name,)+) {}
        impl<$($name: Clone + 'static),+> CloneBundle for ($($name,)+) {
            type Borrowed<'world> = ($(Ref<'world, $name>,)+);
            fn borrow(world: &World, entity: Entity) -> Result<Self::Borrowed<'_>, Error> {
                Ok(($(world.get::<&$name>(entity)?,)+))
            }
            fn clone_components(components: &Self::Borrowed<'_>) -> Self {
                ($(<$name as Clone>::clone(&components.$index),)+)
            }
        }
    }
}
bundle!(A: 0);
bundle!(A: 0, B: 1);
bundle!(A: 0, B: 1, C: 2);
bundle!(A: 0, B: 1, C: 2, D: 3);
bundle!(A: 0, B: 1, C: 2, D: 3, E: 4);
bundle!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5);
bundle!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6);
bundle!(A: 0, B: 1, C: 2, D: 3, E: 4, F: 5, G: 6, H: 7);

fn validate_bundle<B: Bundle>() -> Result<(), Error> {
    let types = B::component_types_inline();
    for (index, item) in types.iter().enumerate() {
        if let Some((id, name)) = item
            && types[..index].iter().flatten().any(|other| other.0 == *id)
        {
            return Err(Error::DuplicateComponent(name));
        }
    }
    Ok(())
}

#[doc(hidden)]
pub struct AccessInfo {
    id: TypeId,
    name: &'static str,
    mutable: bool,
}

/// A required (`&T`, `&mut T`) or optional (`Option<&T>`, `Option<&mut T>`)
/// component access, or the entity identity itself.
///
/// Optional accesses return optional borrow guards, not naked references. They
/// match every live entity, returning `None` only for absent components. See
/// [`World::get`] and [`World::query`] for runnable examples.
pub trait Access: sealed::Access {
    type Item<'world>;
    #[doc(hidden)]
    fn info() -> Option<AccessInfo>;
    #[doc(hidden)]
    fn matches(world: &World, entity: Entity) -> bool;
    #[doc(hidden)]
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error>;
}

impl<T: 'static> sealed::Access for &T {}
impl<T: 'static> Access for &T {
    type Item<'world> = Ref<'world, T>;
    fn info() -> Option<AccessInfo> {
        Some(AccessInfo {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
            mutable: false,
        })
    }
    fn matches(world: &World, entity: Entity) -> bool {
        world.has::<T>(entity)
    }
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
        world.validate(entity)?;
        world
            .cell::<T>(entity.index as usize)
            .ok_or(Error::MissingComponent(type_name::<T>()))?
            .try_borrow()
            .map_err(|_| Error::BorrowConflict(type_name::<T>()))
    }
}
impl<T: 'static> sealed::Access for &mut T {}
impl<T: 'static> Access for &mut T {
    type Item<'world> = RefMut<'world, T>;
    fn info() -> Option<AccessInfo> {
        Some(AccessInfo {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
            mutable: true,
        })
    }
    fn matches(world: &World, entity: Entity) -> bool {
        world.has::<T>(entity)
    }
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
        world.validate(entity)?;
        world
            .cell::<T>(entity.index as usize)
            .ok_or(Error::MissingComponent(type_name::<T>()))?
            .try_borrow_mut()
            .map_err(|_| Error::BorrowConflict(type_name::<T>()))
    }
}
impl<T: 'static> sealed::Access for Option<&T> {}
impl<T: 'static> Access for Option<&T> {
    type Item<'world> = Option<Ref<'world, T>>;
    fn info() -> Option<AccessInfo> {
        // Optionality changes matching, never the aliasing rules for a type.
        <&T as Access>::info()
    }
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity)
    }
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
        world.validate(entity)?;
        world
            .cell::<T>(entity.index as usize)
            .map(|cell| {
                cell.try_borrow()
                    .map_err(|_| Error::BorrowConflict(type_name::<T>()))
            })
            .transpose()
    }
}
impl<T: 'static> sealed::Access for Option<&mut T> {}
impl<T: 'static> Access for Option<&mut T> {
    type Item<'world> = Option<RefMut<'world, T>>;
    fn info() -> Option<AccessInfo> {
        <&mut T as Access>::info()
    }
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity)
    }
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
        world.validate(entity)?;
        world
            .cell::<T>(entity.index as usize)
            .map(|cell| {
                cell.try_borrow_mut()
                    .map_err(|_| Error::BorrowConflict(type_name::<T>()))
            })
            .transpose()
    }
}
impl sealed::Access for Entity {}
impl Access for Entity {
    type Item<'world> = Entity;
    fn info() -> Option<AccessInfo> {
        None
    }
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity)
    }
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
        world.validate(entity)?;
        Ok(entity)
    }
}

/// Query signature: one access type or a tuple of one to eight access types.
/// Each [`Entity`] counts as an entry. Filter clauses have their own separate
/// four-clause group bound; see [`QueryFilter`]. See [`Bundle`] for an actor system.
///
/// Eight entries, including an entity identity and optional components:
///
/// ```
/// use ferravex_ecs::{Entity, Error, World};
/// let mut world = World::new();
/// let entity = world.spawn((1_u8, 2_u16, 3_u32, 4_u64, 5_i8, 6_i16, 7_i32, 8_i64));
/// let [(id, mut a, b, c, d, e, f, absent)] = world.get_many::<(
///     Entity, &mut u8, &u16, &u32, &u64, &i8, &i16, Option<&bool>,
/// ), 1>([entity])?;
/// *a += 1;
/// assert_eq!(id, entity);
/// assert_eq!((*a, *b, *c, *d, *e, *f), (2, 2, 3, 4, 5, 6));
/// assert!(absent.is_none());
/// # Ok::<(), Error>(())
/// ```
///
/// Entity plus eight component accesses exceeds the fetched tuple ceiling:
///
/// ```compile_fail
/// use ferravex_ecs::{Entity, World};
/// World::new().query::<(Entity, &u8, &u16, &u32, &u64, &i8, &i16, &i32, &i64)>();
/// ```
pub trait Query: sealed::Query {
    type Item<'world>;
    #[doc(hidden)]
    fn validate() -> Result<(), Error>;
    #[doc(hidden)]
    fn matches(world: &World, entity: Entity) -> bool;
    #[doc(hidden)]
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error>;
}

impl<A: Access> sealed::Query for A {}
impl<A: Access> Query for A {
    type Item<'world> = A::Item<'world>;
    fn validate() -> Result<(), Error> {
        Ok(())
    }
    fn matches(world: &World, entity: Entity) -> bool {
        A::matches(world, entity)
    }
    fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
        A::fetch(world, entity)
    }
}

macro_rules! query_tuple {
    ($($name:ident),+) => {
        impl<$($name: Access),+> sealed::Query for ($($name,)+) {}
        impl<$($name: Access),+> Query for ($($name,)+) {
            type Item<'world> = ($($name::Item<'world>,)+);
            fn validate() -> Result<(), Error> {
                let access = [$($name::info(),)+];
                for (index, item) in access.iter().enumerate() {
                    if let Some(item) = item {
                        if access[..index].iter().flatten().any(|other| {
                            other.id == item.id && (other.mutable || item.mutable)
                        }) { return Err(Error::ConflictingQuery(item.name)); }
                    }
                }
                Ok(())
            }
            fn matches(world: &World, entity: Entity) -> bool {
                $($name::matches(world, entity))&&+
            }
            fn fetch(world: &World, entity: Entity) -> Result<Self::Item<'_>, Error> {
                Ok(($($name::fetch(world, entity)?,)+))
            }
        }
    }
}
query_tuple!(A);
query_tuple!(A, B);
query_tuple!(A, B, C);
query_tuple!(A, B, C, D);
query_tuple!(A, B, C, D, E);
query_tuple!(A, B, C, D, E, F);
query_tuple!(A, B, C, D, E, F, G);
query_tuple!(A, B, C, D, E, F, G, H);

/// A sealed structural component-presence predicate for [`World::query_filtered`].
///
/// Filters are types, not values or component accesses. `With<T>` and
/// `Without<T>` test whether a live entity has a component of exactly type `T`.
/// They do not inspect tag values (see the engine's separate `tags::TagPredicate`).
/// Tuples and [`All`] require every clause; [`Any`] requires at least one;
/// [`Not`] negates a predicate. Groups have zero to four clauses and may nest.
/// Empty conjunctions (`()`, `All<()>`) are true; `Any<()>` is false. Negation
/// applies within the live entity universe; dead slots are always excluded.
/// See [`World::query_filtered`] for a runnable tick-system recipe.
///
/// ```
/// use ferravex_ecs::{All, Any, Entity, Not, With, Without, World};
/// let mut world = World::new();
/// let tagged = world.spawn((1_u32,));
/// world.spawn(());
/// assert_eq!(world.query_filtered::<Entity, All<(With<u32>, Without<bool>)>>()
///     .iter().collect::<Vec<_>>(), [tagged]);
/// assert_eq!(world.query_filtered::<Entity, Any<()>>().iter().count(), 0);
/// assert_eq!(world.query_filtered::<Entity, Not<Any<()>>>().iter().count(), 2);
/// ```
///
/// Each tuple group is bounded to four clauses. Nest a group to combine more:
///
/// ```compile_fail
/// use ferravex_ecs::{Any, Entity, With, World};
/// let world = World::new();
/// world.query_filtered::<Entity, Any<(
///     With<u8>, With<u16>, With<u32>, With<u64>, With<bool>
/// )>>(); // Five clauses in one group are unsupported.
/// ```
pub trait QueryFilter: sealed::QueryFilter {
    #[doc(hidden)]
    fn matches(world: &World, entity: Entity) -> bool;
}

/// Presence of component `T`, without borrowing its value.
/// Used as a type argument; see [`World::query_filtered`] for a runnable example.
pub struct With<T: 'static>(PhantomData<fn() -> T>);

/// Absence of component `T`, without borrowing any value.
/// Used as a type argument; see [`QueryFilter`] for a runnable example.
pub struct Without<T: 'static>(PhantomData<fn() -> T>);

/// Conjunction of a tuple of zero to four filters; groups may nest.
/// `All<()>` matches every live entity. See [`World::query_filtered`].
pub struct All<F>(PhantomData<fn() -> F>);

/// Disjunction of a tuple of zero to four filters; groups may nest.
/// `Any<()>` matches no entities. See [`QueryFilter`] for a runnable example.
pub struct Any<F>(PhantomData<fn() -> F>);

/// Negation of one filter within live entities. See [`QueryFilter`].
pub struct Not<F: QueryFilter>(PhantomData<fn() -> F>);

impl<T: 'static> sealed::QueryFilter for With<T> {}
impl<T: 'static> QueryFilter for With<T> {
    fn matches(world: &World, entity: Entity) -> bool {
        world.has::<T>(entity)
    }
}
impl<T: 'static> sealed::QueryFilter for Without<T> {}
impl<T: 'static> QueryFilter for Without<T> {
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity) && !world.has::<T>(entity)
    }
}
impl<F: QueryFilter> sealed::QueryFilter for Not<F> {}
impl<F: QueryFilter> QueryFilter for Not<F> {
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity) && !F::matches(world, entity)
    }
}
impl sealed::QueryFilter for () {}
impl QueryFilter for () {
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity)
    }
}
impl sealed::QueryFilter for All<()> {}
impl QueryFilter for All<()> {
    fn matches(world: &World, entity: Entity) -> bool {
        world.contains(entity)
    }
}
impl sealed::QueryFilter for Any<()> {}
impl QueryFilter for Any<()> {
    fn matches(_: &World, _: Entity) -> bool {
        false
    }
}

macro_rules! filter_tuple {
    ($($name:ident),+) => {
        impl<$($name: QueryFilter),+> sealed::QueryFilter for ($($name,)+) {}
        impl<$($name: QueryFilter),+> QueryFilter for ($($name,)+) {
            fn matches(world: &World, entity: Entity) -> bool {
                $($name::matches(world, entity))&&+
            }
        }
        impl<$($name: QueryFilter),+> sealed::QueryFilter for All<($($name,)+)> {}
        impl<$($name: QueryFilter),+> QueryFilter for All<($($name,)+)> {
            fn matches(world: &World, entity: Entity) -> bool {
                <($($name,)+) as QueryFilter>::matches(world, entity)
            }
        }
        impl<$($name: QueryFilter),+> sealed::QueryFilter for Any<($($name,)+)> {}
        impl<$($name: QueryFilter),+> QueryFilter for Any<($($name,)+)> {
            fn matches(world: &World, entity: Entity) -> bool {
                $($name::matches(world, entity))||+
            }
        }
    }
}
filter_tuple!(A);
filter_tuple!(A, B);
filter_tuple!(A, B, C);
filter_tuple!(A, B, C, D);

pub struct QueryBorrow<'world, Q: Query, F: QueryFilter = ()> {
    world: &'world World,
    marker: PhantomData<fn() -> (Q, F)>,
}

impl<'world, Q: Query, F: QueryFilter> QueryBorrow<'world, Q, F> {
    pub fn iter(&self) -> QueryIter<'world, Q, F> {
        QueryIter {
            inner: self.try_iter(),
        }
    }
    /// Visits all matching entities, reporting borrow conflicts without skipping silently.
    /// A conflict releases any partial tuple borrows and does not stop later entities.
    pub fn try_iter(&self) -> TryQueryIter<'world, Q, F> {
        TryQueryIter {
            world: self.world,
            cursor: 0,
            marker: PhantomData,
        }
    }
}
impl<'world, Q: Query, F: QueryFilter> IntoIterator for &mut QueryBorrow<'world, Q, F> {
    type Item = Q::Item<'world>;
    type IntoIter = QueryIter<'world, Q, F>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'world, Q: Query, F: QueryFilter> IntoIterator for &QueryBorrow<'world, Q, F> {
    type Item = Q::Item<'world>;
    type IntoIter = QueryIter<'world, Q, F>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'world, Q: Query, F: QueryFilter> IntoIterator for QueryBorrow<'world, Q, F> {
    type Item = Q::Item<'world>;
    type IntoIter = QueryIter<'world, Q, F>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct TryQueryIter<'world, Q: Query, F: QueryFilter = ()> {
    world: &'world World,
    cursor: usize,
    marker: PhantomData<fn() -> (Q, F)>,
}
impl<'world, Q: Query, F: QueryFilter> Iterator for TryQueryIter<'world, Q, F> {
    type Item = Result<Q::Item<'world>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(slot) = self.world.slots.get(self.cursor) {
            let entity = Entity {
                index: self.cursor as u32,
                generation: slot.generation,
            };
            self.cursor += 1;
            // Match structural filters and the entire tuple before any borrow.
            if slot.alive && F::matches(self.world, entity) && Q::matches(self.world, entity) {
                return Some(Q::fetch(self.world, entity));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.world.slots.len().saturating_sub(self.cursor)))
    }
}
impl<Q: Query, F: QueryFilter> std::iter::FusedIterator for TryQueryIter<'_, Q, F> {}

pub struct QueryIter<'world, Q: Query, F: QueryFilter = ()> {
    inner: TryQueryIter<'world, Q, F>,
}
impl<'world, Q: Query, F: QueryFilter> Iterator for QueryIter<'world, Q, F> {
    type Item = Q::Item<'world>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|item| item.unwrap_or_else(|error| panic!("{error}")))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
impl<Q: Query, F: QueryFilter> std::iter::FusedIterator for QueryIter<'_, Q, F> {}

#[cfg(test)]
mod journey400_212 {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Part<const N: usize>(i32);

    type Eight = (
        Part<0>,
        Part<1>,
        Part<2>,
        Part<3>,
        Part<4>,
        Part<5>,
        Part<6>,
        Part<7>,
    );
    type MutableEight = (
        &'static mut Part<0>,
        &'static mut Part<1>,
        &'static mut Part<2>,
        &'static mut Part<3>,
        &'static mut Part<4>,
        &'static mut Part<5>,
        &'static mut Part<6>,
        &'static mut Part<7>,
    );

    fn eight(value: i32) -> Eight {
        (
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
        )
    }

    #[test]
    fn stack_metadata_and_existing_vec_api_agree_for_every_bundle_width() {
        fn check<B: Bundle>(expected: &[(TypeId, &'static str)]) {
            let inline = B::component_types_inline();
            assert_eq!(
                inline.iter().flatten().copied().collect::<Vec<_>>(),
                expected
            );
            assert!(inline[expected.len()..].iter().all(Option::is_none));
            // The old hidden public API still appends and preserves existing entries.
            let prefix = (TypeId::of::<()>(), type_name::<()>());
            let mut metadata = vec![prefix];
            B::component_types(&mut metadata);
            assert_eq!(metadata[0], prefix);
            assert_eq!(&metadata[1..], expected);
            assert_eq!(validate_bundle::<B>(), Ok(()));
        }
        check::<()>(&[]);
        macro_rules! check {
            ($($ty:ty),+) => {
                check::<($($ty,)+)>(&[$((TypeId::of::<$ty>(), type_name::<$ty>()),)+]);
            };
        }
        check!(u8);
        check!(u8, u16);
        check!(u8, u16, u32);
        check!(u8, u16, u32, u64);
        check!(u8, u16, u32, u64, i8);
        check!(u8, u16, u32, u64, i8, i16);
        check!(u8, u16, u32, u64, i8, i16, i32);
        check!(u8, u16, u32, u64, i8, i16, i32, i64);
    }

    #[test]
    fn new_tuple_widths_support_spawn_insert_query_and_get_many() {
        macro_rules! check {
            ($($ty:ty => $value:expr),+) => {{
                let mut world = World::new();
                let entity = world.try_spawn(($($value,)+)).unwrap();
                assert_eq!(world.query::<($(&$ty,)+)>().iter().count(), 1);
                assert_eq!(world.get_many::<($(&$ty,)+), 1>([entity]).unwrap().len(), 1);
                world.insert(entity, ($($value,)+)).unwrap();
                $(assert_eq!(*world.get::<&$ty>(entity).unwrap(), $value);)+
                world.despawn(entity).unwrap();
                assert_eq!(world.query::<($(&$ty,)+)>().iter().count(), 0);
            }};
        }
        check!(u8 => 1_u8, u16 => 2_u16, u32 => 3_u32, u64 => 4_u64, i8 => 5_i8);
        check!(u8 => 1_u8, u16 => 2_u16, u32 => 3_u32, u64 => 4_u64, i8 => 5_i8, i16 => 6_i16);
        check!(u8 => 1_u8, u16 => 2_u16, u32 => 3_u32, u64 => 4_u64, i8 => 5_i8, i16 => 6_i16, i32 => 7_i32);
        check!(u8 => 1_u8, u16 => 2_u16, u32 => 3_u32, u64 => 4_u64, i8 => 5_i8, i16 => 6_i16, i32 => 7_i32, i64 => 8_i64);
    }

    #[test]
    fn late_duplicate_bundles_prevalidate_without_writes_or_slot_changes() {
        fn rejects<B: Bundle>(world: &mut World, entity: Entity, bundle: B) {
            let slots = world.slots.len();
            let free = world.free.clone();
            let storages = world.storages.len();
            let len = world.len();
            assert_eq!(
                world.try_spawn(bundle),
                Err(Error::DuplicateComponent(type_name::<Part<0>>()))
            );
            assert_eq!(world.slots.len(), slots);
            assert_eq!(world.free, free);
            assert_eq!(world.storages.len(), storages);
            assert_eq!(world.len(), len);
            assert_eq!(world.get::<&Part<0>>(entity).unwrap().0, 10);
        }
        let mut world = World::new();
        let entity = world.spawn((Part::<0>(10),));
        let freed = world.spawn(());
        world.despawn(freed).unwrap();
        rejects(
            &mut world,
            entity,
            (
                Part::<0>(1),
                Part::<1>(1),
                Part::<2>(1),
                Part::<3>(1),
                Part::<0>(2),
            ),
        );
        rejects(
            &mut world,
            entity,
            (
                Part::<0>(1),
                Part::<1>(1),
                Part::<2>(1),
                Part::<3>(1),
                Part::<4>(1),
                Part::<0>(2),
            ),
        );
        rejects(
            &mut world,
            entity,
            (
                Part::<0>(1),
                Part::<1>(1),
                Part::<2>(1),
                Part::<3>(1),
                Part::<4>(1),
                Part::<5>(1),
                Part::<0>(2),
            ),
        );
        let duplicate = || {
            (
                Part::<0>(1),
                Part::<1>(1),
                Part::<2>(1),
                Part::<3>(1),
                Part::<4>(1),
                Part::<5>(1),
                Part::<6>(1),
                Part::<0>(2),
            )
        };
        rejects(&mut world, entity, duplicate());
        assert_eq!(
            world.insert(entity, duplicate()),
            Err(Error::DuplicateComponent(type_name::<Part<0>>()))
        );
        assert_eq!(world.get::<&Part<0>>(entity).unwrap().0, 10);
        assert!(!world.has::<Part<1>>(entity));
        assert_eq!(world.storages.len(), 1);
        assert_eq!(
            world.insert(freed, duplicate()),
            Err(Error::NoSuchEntity(freed))
        );
        let panic =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.spawn(duplicate())));
        assert!(panic.is_err());
        let next = world.spawn(());
        assert_eq!(next.id(), freed.id());
        assert_eq!(next.generation(), freed.generation() + 1);
        assert_eq!(
            validate_bundle::<(u8, u16, u32, u64, u8, i16, i32, u16)>(),
            Err(Error::DuplicateComponent(type_name::<u8>()))
        );
    }

    #[test]
    fn late_query_aliases_validate_on_empty_worlds_filters_and_batches() {
        fn rejects<Q: Query>() {
            let world = World::new();
            let error = Error::ConflictingQuery(type_name::<Part<0>>());
            assert_eq!(world.try_query::<Q>().err(), Some(error.clone()));
            assert_eq!(
                world.try_query_filtered::<Q, Any<()>>().err(),
                Some(error.clone())
            );
            assert_eq!(world.get_many::<Q, 0>([]).err(), Some(error.clone()));
            assert_eq!(
                world.get_many::<Q, 1>([Entity::from_bits(u64::MAX)]).err(),
                Some(error)
            );
        }
        rejects::<(&mut Part<0>, Entity, Entity, Entity, &Part<0>)>();
        rejects::<(
            &Part<0>,
            Entity,
            Entity,
            Entity,
            Entity,
            Option<&mut Part<0>>,
        )>();
        rejects::<(
            Option<&mut Part<0>>,
            Entity,
            Entity,
            Entity,
            Entity,
            Entity,
            Option<&Part<0>>,
        )>();
        rejects::<(
            Entity,
            Entity,
            Entity,
            Entity,
            Entity,
            Entity,
            &mut Part<0>,
            Option<&mut Part<0>>,
        )>();
        assert_eq!(
            <(
                Entity,
                &mut Part<1>,
                Entity,
                Entity,
                &Part<0>,
                Entity,
                &mut Part<1>,
                &mut Part<0>
            ) as Query>::validate(),
            Err(Error::ConflictingQuery(type_name::<Part<1>>()))
        );
    }

    #[test]
    fn eight_shared_or_identity_entries_remain_valid() {
        let mut world = World::new();
        let entity = world.spawn((Part::<0>(3),));
        let [(id, a, b, c, d, e, f, g)] = world
            .get_many::<(
                Entity,
                &Part<0>,
                Option<&Part<0>>,
                &Part<0>,
                &Part<0>,
                &Part<0>,
                &Part<0>,
                &Part<0>,
            ), 1>([entity])
            .unwrap();
        assert_eq!(id, entity);
        assert_eq!(
            (a.0, b.unwrap().0, c.0, d.0, e.0, f.0, g.0),
            (3, 3, 3, 3, 3, 3, 3)
        );
        assert_eq!(
            world
                .query::<(
                    Entity,
                    Entity,
                    Entity,
                    Entity,
                    Entity,
                    Entity,
                    Entity,
                    Entity
                )>()
                .iter()
                .next()
                .unwrap(),
            (
                entity, entity, entity, entity, entity, entity, entity, entity
            )
        );
    }

    #[test]
    fn eighth_borrow_failure_releases_seven_guards_and_iteration_continues() {
        let mut world = World::new();
        let first = world.spawn(eight(1));
        let second = world.spawn(eight(2));
        let held = world.get_mut::<Part<7>>(first).unwrap();
        let mut rows = world.query::<MutableEight>().try_iter();
        assert!(
            matches!(rows.next().unwrap(), Err(Error::BorrowConflict(name)) if name == type_name::<Part<7>>())
        );
        macro_rules! released {
            ($($index:literal),+) => { $(assert!(world.get_mut::<Part<$index>>(first).is_ok());)+ };
        }
        released!(0, 1, 2, 3, 4, 5, 6);
        assert_eq!(held.0, 1);
        let (mut a, b, c, d, e, f, g, h) = rows.next().unwrap().unwrap();
        a.0 += 10;
        assert_eq!((b.0, c.0, d.0, e.0, f.0, g.0, h.0), (2, 2, 2, 2, 2, 2, 2));
        assert!(rows.next().is_none());
        assert!(rows.next().is_none());
        drop((a, b, c, d, e, f, g, h, held));
        assert_eq!(world.get::<&Part<0>>(second).unwrap().0, 12);
        assert!(world.get_many::<MutableEight, 2>([first, second]).is_ok());
        let held = world.get_mut::<Part<7>>(first).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.query::<MutableEight>().iter().next();
        }));
        assert!(panic.is_err());
        released!(0, 1, 2, 3, 4, 5, 6);
        drop(held);
        assert!(world.get_many::<MutableEight, 1>([first]).is_ok());
    }

    #[test]
    fn wide_get_many_cleans_completed_and_partial_rows_on_late_errors() {
        let mut world = World::new();
        let first = world.spawn(eight(1));
        let last = world.spawn(eight(2));
        world.remove::<Part<7>>(last).unwrap();
        assert!(matches!(world.get_many::<MutableEight, 2>([first, last]),
            Err(Error::MissingComponent(name)) if name == type_name::<Part<7>>()));
        assert!(world.get_many::<MutableEight, 1>([first]).is_ok());
        assert!(world.get_mut::<Part<0>>(last).is_ok());
        assert!(world.get_mut::<Part<6>>(last).is_ok());
        world.insert(last, (Part::<7>(2),)).unwrap();
        let held = world.get::<&Part<7>>(last).unwrap();
        assert!(matches!(world.get_many::<MutableEight, 2>([first, last]),
            Err(Error::BorrowConflict(name)) if name == type_name::<Part<7>>()));
        assert!(world.get_many::<MutableEight, 1>([first]).is_ok());
        assert!(world.get_mut::<Part<6>>(last).is_ok());
        assert_eq!(held.0, 2);
        drop(held);
        assert!(world.get_many::<MutableEight, 2>([last, first]).is_ok());
        assert!(matches!(
            world.get_many::<MutableEight, 2>([first, first]),
            Err(Error::BorrowConflict(_))
        ));
        assert!(world.get_many::<MutableEight, 1>([first]).is_ok());
        assert_eq!(world.get::<&Part<0>>(first).unwrap().0, 1);
    }

    #[test]
    fn wide_optional_filtered_queries_prefilter_and_never_borrow_markers() {
        type Fetch = (
            Entity,
            &'static mut Part<0>,
            Option<&'static Part<1>>,
            &'static Part<2>,
            &'static Part<3>,
            &'static Part<4>,
            &'static Part<5>,
            Option<&'static mut Part<6>>,
        );
        let mut world = World::new();
        let excluded = world.spawn(eight(1));
        let missing_required = world.spawn(eight(2));
        world.remove::<Part<5>>(missing_required).unwrap();
        let full = world.spawn(eight(3));
        let partial = world.spawn(eight(4));
        world.remove::<Part<1>>(partial).unwrap();
        world.remove::<Part<6>>(partial).unwrap();
        world.insert(excluded, (false,)).unwrap();
        let excluded_guard = world.get_mut::<Part<0>>(excluded).unwrap();
        let missing_guard = world.get_mut::<Part<0>>(missing_required).unwrap();
        let marker = world.get_mut::<Part<7>>(full).unwrap();
        let mut visited = Vec::new();
        for row in world
            .query_filtered::<Fetch, (With<Part<7>>, Without<bool>)>()
            .try_iter()
        {
            let (id, mut a, b, c, d, e, f, g) = row.unwrap();
            assert_eq!((c.0, d.0, e.0, f.0), (a.0, a.0, a.0, a.0));
            assert_eq!(b.is_some(), id == full);
            assert_eq!(g.is_some(), id == full);
            a.0 += 10;
            if let Some(mut g) = g {
                g.0 += 20;
            }
            visited.push(id);
        }
        assert_eq!(visited, [full, partial]);
        assert_eq!(marker.0, 3);
        drop((marker, excluded_guard, missing_guard));
        assert_eq!(world.get::<&Part<0>>(full).unwrap().0, 13);
        assert_eq!(world.get::<&Part<6>>(full).unwrap().0, 23);
        assert_eq!(world.get::<&Part<0>>(partial).unwrap().0, 14);
        // Optional eighth fetches still report a present component's borrow failure.
        let held = world.get_mut::<Part<6>>(full).unwrap();
        let mut rows = world.query_filtered::<Fetch, Without<bool>>().try_iter();
        assert!(
            matches!(rows.next().unwrap(), Err(Error::BorrowConflict(name)) if name == type_name::<Part<6>>())
        );
        assert!(world.get_mut::<Part<0>>(full).is_ok());
        assert_eq!(rows.next().unwrap().unwrap().0, partial);
        assert!(rows.next().is_none());
        drop(held);
    }

    #[test]
    fn wide_bundles_reuse_warmed_buffers_and_reset_through_despawn() {
        let mut world = World::new();
        let initial = world.spawn(eight(0));
        world.despawn(initial).unwrap(); // Warm the free-list and all eight storages.
        let capacities = (
            world.slots.capacity(),
            world.free.capacity(),
            world.storages.capacity(),
        );
        fn component_capacities(world: &World) -> [usize; 8] {
            macro_rules! capacities {
                ($($index:literal),+) => {
                    [$(world.storages[&TypeId::of::<Part<$index>>()]
                        .as_any().downcast_ref::<Storage<Part<$index>>>()
                        .unwrap().slots.capacity(),)+]
                };
            }
            capacities!(0, 1, 2, 3, 4, 5, 6, 7)
        }
        let storage_capacities = component_capacities(&world);
        let mut old = initial;
        for value in 1..=64 {
            let entity = world.try_spawn(eight(value)).unwrap();
            assert_eq!(entity.id(), initial.id());
            assert!(!world.contains(old));
            assert_eq!(
                world.get_many::<MutableEight, 1>([old]).err(),
                Some(Error::NoSuchEntity(old))
            );
            assert_eq!(world.get::<&Part<7>>(entity).unwrap().0, value);
            world.insert(entity, ()).unwrap();
            world.despawn(entity).unwrap();
            assert!(world.is_empty());
            assert_eq!(world.query::<MutableEight>().iter().count(), 0);
            old = entity;
        }
        assert_eq!(
            (
                world.slots.capacity(),
                world.free.capacity(),
                world.storages.capacity()
            ),
            capacities
        );
        assert_eq!(world.storages.len(), 8);
        assert_eq!(component_capacities(&world), storage_capacities);
        let empty = world.try_spawn(()).unwrap();
        assert_eq!(empty.id(), initial.id());
        assert!(world.get::<Option<&Part<0>>>(empty).unwrap().is_none());
        assert!(world.get::<Option<&Part<7>>>(empty).unwrap().is_none());
        assert_eq!(world.query::<Entity>().iter().collect::<Vec<_>>(), [empty]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Debug, PartialEq)]
    struct Position(f32, f32);
    #[derive(Debug, PartialEq)]
    struct Velocity(f32, f32);
    #[derive(Debug, PartialEq)]
    struct Health(i32);

    fn filtered_ids<F: QueryFilter>(world: &World) -> Vec<Entity> {
        world.query_filtered::<Entity, F>().iter().collect()
    }

    #[test]
    fn filtered_logic_matches_all_component_presence_combinations() {
        let mut world = World::new();
        let entities: Vec<_> = (0..16)
            .map(|mask| {
                let entity = world.spawn(());
                if mask & 1 != 0 {
                    world.insert(entity, (Health(1),)).unwrap();
                }
                if mask & 2 != 0 {
                    world.insert(entity, (Position(0.0, 0.0),)).unwrap();
                }
                if mask & 4 != 0 {
                    world.insert(entity, (Velocity(0.0, 0.0),)).unwrap();
                }
                if mask & 8 != 0 {
                    world.insert(entity, (true,)).unwrap();
                }
                entity
            })
            .collect();
        macro_rules! check {
            ($filter:ty, $predicate:expr) => {
                assert_eq!(
                    filtered_ids::<$filter>(&world),
                    entities
                        .iter()
                        .enumerate()
                        .filter_map(|(mask, &entity)| { ($predicate)(mask).then_some(entity) })
                        .collect::<Vec<_>>()
                );
            };
        }
        check!(With<Health>, |m: usize| m & 1 != 0);
        check!(Without<Health>, |m: usize| m & 1 == 0);
        check!((With<Health>,), |m: usize| m & 1 != 0);
        check!((With<Health>, With<Position>), |m: usize| m & 3 == 3);
        check!(
            (With<Health>, With<Position>, With<Velocity>),
            |m: usize| m & 7 == 7
        );
        check!(
            (With<Health>, With<Position>, With<Velocity>, With<bool>),
            |m| m == 15
        );
        check!(All<(With<Health>,)>, |m: usize| m & 1 != 0);
        check!(All<(With<Health>, With<Position>)>, |m: usize| m & 3 == 3);
        check!(
            All<(With<Health>, With<Position>, With<Velocity>)>,
            |m: usize| m & 7 == 7
        );
        check!(
            All<(With<Health>, With<Position>, With<Velocity>, With<bool>)>,
            |m| m == 15
        );
        check!(Any<(With<Health>,)>, |m: usize| m & 1 != 0);
        check!(Any<(With<Health>, With<Position>)>, |m: usize| m & 3 != 0);
        check!(
            Any<(With<Health>, With<Position>, With<Velocity>)>,
            |m: usize| m & 7 != 0
        );
        check!(
            Any<(With<Health>, With<Position>, With<Velocity>, With<bool>)>,
            |m| m != 0
        );
        type Nested = All<(
            Any<(With<Health>, With<Position>)>,
            Not<Any<(With<Velocity>, With<bool>)>>,
        )>;
        check!(Nested, |m: usize| m & 3 != 0 && m & 12 == 0);
        check!(Not<Nested>, |m: usize| m & 3 == 0 || m & 12 != 0);
        check!(Not<Not<With<Health>>>, |m: usize| m & 1 != 0);
        check!((With<Health>, Without<Health>), |_| false);
    }

    #[test]
    fn filtered_empty_groups_have_logical_identities() {
        let mut world = World::new();
        assert!(filtered_ids::<Not<Any<()>>>(&world).is_empty());
        let empty = world.spawn(());
        let present = world.spawn((Health(1),));
        let dead = world.spawn(());
        world.despawn(dead).unwrap();
        for ids in [
            filtered_ids::<()>(&world),
            filtered_ids::<All<()>>(&world),
            filtered_ids::<Not<Any<()>>>(&world),
            filtered_ids::<Any<(With<Health>, Without<Health>)>>(&world),
        ] {
            assert_eq!(ids, [empty, present]);
        }
        assert!(filtered_ids::<Any<()>>(&world).is_empty());
        assert!(filtered_ids::<Not<()>>(&world).is_empty());
        assert!(filtered_ids::<Not<All<()>>>(&world).is_empty());
        assert_eq!(filtered_ids::<All<((), With<Health>)>>(&world), [present]);
        assert_eq!(
            filtered_ids::<Any<(Any<()>, Without<Health>)>>(&world),
            [empty]
        );
    }

    #[test]
    fn filtered_presence_never_borrows_marker_values() {
        struct Marker;
        let mut world = World::new();
        let marked = world.spawn((Marker, Health(10)));
        let unmarked = world.spawn((Health(20),));
        let marker = world.get_mut::<Marker>(marked).unwrap();
        assert_eq!(filtered_ids::<With<Marker>>(&world), [marked]);
        assert_eq!(filtered_ids::<Without<Marker>>(&world), [unmarked]);
        assert_eq!(filtered_ids::<Not<Without<Marker>>>(&world), [marked]);
        let mut health = world
            .query_filtered::<&mut Health, With<Marker>>()
            .iter()
            .next()
            .unwrap();
        health.0 += 1;
        // Filtering never leaves a marker guard behind, even while a fetched guard lives.
        drop(marker);
        assert!(world.get_mut::<Marker>(marked).is_ok());
        drop(health);
        // The very same type may be a mutable fetch and repeated presence clause.
        for _marker in world.query_filtered::<&mut Marker, (With<Marker>, With<Marker>)>() {}
        assert_eq!(world.get::<&Health>(marked).unwrap().0, 11);
    }

    #[test]
    fn filtered_required_optional_accesses_and_exclusions_match_before_fetch() {
        let mut world = World::new();
        let excluded = world.spawn((Health(10), Position(1.0, 2.0), true));
        let missing_required = world.spawn((Health(20),));
        let included = world.spawn((Position(3.0, 4.0),));
        let excluded_guard = world.get_mut::<Health>(excluded).unwrap();
        let missing_guard = world.get_mut::<Health>(missing_required).unwrap();
        let mut rows = world
            .query_filtered::<(
                Entity,
                Option<&mut Health>,
                &mut Position,
                Option<&Velocity>,
            ), Without<bool>>()
            .try_iter();
        let (id, health, mut position, velocity) = rows.next().unwrap().unwrap();
        assert_eq!(id, included);
        assert!(health.is_none() && velocity.is_none());
        position.0 += 5.0;
        assert!(rows.next().is_none());
        drop((excluded_guard, missing_guard, position));
        assert_eq!(world.get::<&Position>(included).unwrap().0, 8.0);
        // Optional-only fetches still respect structural exclusion.
        assert_eq!(
            world
                .query_filtered::<Option<&Velocity>, With<bool>>()
                .iter()
                .count(),
            1
        );
    }

    #[test]
    fn filtered_alias_validation_is_independent_of_filter_matches() {
        fn rejects<F: QueryFilter>(world: &World) {
            for result in [
                world
                    .try_query_filtered::<(&mut Health, Option<&Health>), F>()
                    .map(|_| ()),
                world
                    .try_query_filtered::<(Option<&mut Health>, &mut Health), F>()
                    .map(|_| ()),
                world
                    .try_query_filtered::<(Option<&mut Health>, Option<&mut Health>), F>()
                    .map(|_| ()),
            ] {
                assert_eq!(result, Err(Error::ConflictingQuery(type_name::<Health>())));
            }
        }
        let mut world = World::new();
        rejects::<Any<()>>(&world);
        rejects::<(With<Health>, Without<Health>)>(&world);
        world.spawn((Health(1),));
        rejects::<With<Health>>(&world);
        rejects::<Any<()>>(&world);
        let (a, b) = world
            .query_filtered::<(&Health, Option<&Health>), With<Health>>()
            .iter()
            .next()
            .unwrap();
        assert_eq!(a.0, b.unwrap().0);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.query_filtered::<(&mut Health, &Health), Any<()>>();
        }));
        assert!(panic.is_err());
    }

    #[test]
    fn filtered_fallible_iteration_releases_partial_guards_and_continues() {
        let mut world = World::new();
        let first = world.spawn((Health(1), Position(1.0, 2.0), true));
        let excluded = world.spawn((Health(2), Position(3.0, 4.0)));
        let last = world.spawn((Health(3), true));
        let marker = world.get_mut::<bool>(first).unwrap();
        let conflict = world.get_mut::<Position>(first).unwrap();
        let (mut health, position) = {
            let mut rows = world
                .query_filtered::<(&mut Health, Option<&Position>), With<bool>>()
                .try_iter();
            assert!(matches!(rows.next().unwrap(),
                Err(Error::BorrowConflict(name)) if name == type_name::<Position>()));
            assert!(world.get_mut::<Health>(first).is_ok());
            let last_row = rows.next().unwrap().unwrap();
            assert!(rows.next().is_none());
            assert!(rows.next().is_none());
            assert_eq!(rows.size_hint(), (0, Some(0)));
            last_row
        };
        // Result guards retain the world lifetime after iterator destruction.
        assert!(position.is_none());
        health.0 += 10;
        assert!(matches!(
            world.get::<&Health>(last),
            Err(Error::BorrowConflict(_))
        ));
        assert_eq!(world.get::<&Health>(excluded).unwrap().0, 2);
        drop((health, marker, conflict));
        assert_eq!(
            world
                .query_filtered::<(&mut Health, Option<&Position>), With<bool>>()
                .try_iter()
                .filter(Result::is_ok)
                .count(),
            2
        );
    }

    #[test]
    fn filtered_fetch_of_a_marker_still_reports_borrow_conflicts() {
        let mut world = World::new();
        let present = world.spawn((Health(1),));
        let absent = world.spawn(());
        let held = world.get_mut::<Health>(present).unwrap();
        assert_eq!(filtered_ids::<With<Health>>(&world), [present]);
        let mut rows = world
            .query_filtered::<Option<&Health>, With<Health>>()
            .try_iter();
        assert!(matches!(
            rows.next().unwrap(),
            Err(Error::BorrowConflict(_))
        ));
        assert!(rows.next().is_none());
        assert_eq!(filtered_ids::<Without<Health>>(&world), [absent]);
        drop(held);
        assert!(
            world
                .query_filtered::<Option<&Health>, With<Health>>()
                .try_iter()
                .next()
                .unwrap()
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn filtered_panicking_iteration_releases_partial_guards() {
        let mut world = World::new();
        let entity = world.spawn((Health(1), Position(1.0, 2.0), true));
        let held = world.get_mut::<Position>(entity).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world
                .query_filtered::<(&mut Health, Option<&Position>), With<bool>>()
                .iter()
                .next();
        }));
        assert!(panic.is_err());
        assert!(world.get_mut::<Health>(entity).is_ok());
        drop(held);
        assert!(
            world
                .query_filtered::<(&mut Health, Option<&Position>), With<bool>>()
                .iter()
                .next()
                .is_some()
        );
    }

    #[test]
    fn filtered_membership_tracks_structural_changes_and_slot_reuse() {
        let mut world = World::new();
        let early = world.spawn(());
        let later = world.spawn((Health(2),));
        world.insert(early, (Health(1),)).unwrap();
        assert_eq!(filtered_ids::<With<Health>>(&world), [early, later]);
        world.remove::<Health>(early).unwrap();
        assert_eq!(filtered_ids::<With<Health>>(&world), [later]);
        assert_eq!(filtered_ids::<Without<Health>>(&world), [early]);
        world.despawn(early).unwrap();
        let reused = world.spawn((Health(3),));
        assert_eq!(reused.id(), early.id());
        assert_ne!(reused, early);
        assert_eq!(filtered_ids::<With<Health>>(&world), [reused, later]);
        assert!(!<Not<Without<Health>> as QueryFilter>::matches(
            &world, early
        ));
        world.despawn_many(&[later, reused]).unwrap();
        assert!(filtered_ids::<Not<Any<()>>>(&world).is_empty());
        let restarted = world.spawn(());
        assert!(filtered_ids::<With<Health>>(&world).is_empty());
        assert_eq!(filtered_ids::<Without<Health>>(&world), [restarted]);
    }

    #[test]
    fn filtered_queries_do_not_register_storage_or_grow_buffers() {
        struct NeverRegistered;
        let mut world = World::new();
        world.spawn((Health(1),));
        for _ in 0..127 {
            world.spawn(());
        }
        let before = (
            world.slots.capacity(),
            world.free.capacity(),
            world.storages.capacity(),
            world.storages.len(),
        );
        let health_capacity = world
            .storages
            .get(&TypeId::of::<Health>())
            .unwrap()
            .as_any()
            .downcast_ref::<Storage<Health>>()
            .unwrap()
            .slots
            .capacity();
        type Filter = All<(
            Without<NeverRegistered>,
            Any<(With<Health>, Not<With<Health>>)>,
        )>;
        for _ in 0..4 {
            assert_eq!(
                world
                    .query_filtered::<(Entity, Option<&mut NeverRegistered>), Filter>()
                    .try_iter()
                    .map(|row| {
                        assert!(row.unwrap().1.is_none());
                    })
                    .count(),
                128
            );
            assert_eq!(world.query_filtered::<&Health, Filter>().iter().count(), 1);
            assert!(filtered_ids::<With<NeverRegistered>>(&world).is_empty());
        }
        assert_eq!(
            (
                world.slots.capacity(),
                world.free.capacity(),
                world.storages.capacity(),
                world.storages.len()
            ),
            before
        );
        assert_eq!(
            world
                .storages
                .get(&TypeId::of::<Health>())
                .unwrap()
                .as_any()
                .downcast_ref::<Storage<Health>>()
                .unwrap()
                .slots
                .capacity(),
            health_capacity
        );
        assert!(
            !world
                .storages
                .contains_key(&TypeId::of::<NeverRegistered>())
        );
        assert_eq!(std::mem::size_of::<With<Health>>(), 0);
        assert_eq!(std::mem::size_of::<Filter>(), 0);
        assert_eq!(
            std::mem::size_of::<QueryBorrow<'_, Entity, Filter>>(),
            std::mem::size_of::<QueryBorrow<'_, Entity>>()
        );
        assert_eq!(
            std::mem::size_of::<TryQueryIter<'_, Entity, Filter>>(),
            std::mem::size_of::<TryQueryIter<'_, Entity>>()
        );
    }

    #[test]
    fn filtered_iterator_forms_and_default_type_arguments_remain_usable() {
        fn fused<I: std::iter::FusedIterator>(_: &I) {}
        let mut world = World::new();
        let first = world.spawn((Health(1),));
        world.spawn(());
        let last = world.spawn((Health(3),));
        let unfiltered: QueryBorrow<'_, Entity> = world.try_query().unwrap();
        let _: QueryIter<'_, Entity> = unfiltered.iter();
        let _: TryQueryIter<'_, Entity> = unfiltered.try_iter();
        let mut query: QueryBorrow<'_, Entity, With<Health>> = world.try_query_filtered().unwrap();
        assert_eq!((&query).into_iter().collect::<Vec<_>>(), [first, last]);
        assert_eq!((&mut query).into_iter().collect::<Vec<_>>(), [first, last]);
        assert_eq!(query.into_iter().collect::<Vec<_>>(), [first, last]);
        let mut rows = world.query_filtered::<Entity, With<Health>>().try_iter();
        fused(&rows);
        assert_eq!(rows.size_hint(), (0, Some(3)));
        assert_eq!(rows.next(), Some(Ok(first)));
        assert_eq!(rows.size_hint(), (0, Some(2)));
        assert_eq!(rows.next(), Some(Ok(last)));
        assert_eq!(rows.size_hint(), (0, Some(0)));
        assert_eq!(rows.next(), None);
        assert_eq!(rows.next(), None);
        let iter = world.query_filtered::<Entity, With<Health>>().iter();
        fused(&iter);
    }

    #[test]
    fn reused_slot_rejects_every_stale_handle_operation() {
        let mut world = World::new();
        let old = world.spawn((Position(1.0, 2.0), Health(10)));
        world.despawn(old).unwrap();
        let new = world.spawn((Position(8.0, 9.0),));
        assert_eq!(old.id(), new.id());
        assert_ne!(old.generation(), new.generation());
        assert!(!world.contains(old));
        assert!(world.contains(new));
        assert_eq!(Entity::from_bits(new.to_bits()), new);
        assert!(matches!(
            world.get::<&Position>(old),
            Err(Error::NoSuchEntity(_))
        ));
        assert!(matches!(
            world.get_mut::<Position>(old),
            Err(Error::NoSuchEntity(_))
        ));
        assert!(matches!(
            world.insert(old, (Health(50),)),
            Err(Error::NoSuchEntity(_))
        ));
        assert!(matches!(
            world.remove::<Position>(old),
            Err(Error::NoSuchEntity(_))
        ));
        assert!(matches!(world.despawn(old), Err(Error::NoSuchEntity(_))));
        assert!(!world.has::<Health>(new));
        assert_eq!(*world.get::<&Position>(new).unwrap(), Position(8.0, 9.0));
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn exhausted_generations_retire_slots_instead_of_wrapping() {
        let mut world = World::new();
        let first = world.spawn((7_i32,));
        world.slots[first.id() as usize].generation = u32::MAX;
        let final_handle = Entity {
            index: first.id(),
            generation: u32::MAX,
        };
        world.despawn(final_handle).unwrap();
        let next = world.spawn((8_i32,));
        assert_ne!(next.id(), first.id());
        assert!(!world.contains(first));
        assert!(!world.contains(final_handle));
    }

    #[test]
    fn despawn_many_removes_duplicates_once_in_first_occurrence_order() {
        struct RecordDrop(u32, Rc<RefCell<Vec<u32>>>);
        impl Drop for RecordDrop {
            fn drop(&mut self) {
                self.1.borrow_mut().push(self.0);
            }
        }
        let drops = Rc::new(RefCell::new(Vec::new()));
        let mut world = World::new();
        let first = world.spawn((RecordDrop(1, drops.clone()), Health(10)));
        let survivor = world.spawn((Health(20), Position(2.0, 3.0)));
        let last = world.spawn((RecordDrop(3, drops.clone()), Velocity(4.0, 5.0)));
        let empty = world.spawn(());

        assert_eq!(
            world.despawn_many(&[last, first, last, empty, first]),
            Ok(3)
        );
        assert_eq!(*drops.borrow(), [3, 1]);
        assert_eq!(world.len(), 1);
        assert_eq!(world.entities().collect::<Vec<_>>(), [survivor]);
        assert_eq!(world.query::<&Health>().iter().count(), 1);
        assert_eq!(world.query::<&Velocity>().iter().count(), 0);
        let [(mut health, position)] = world
            .get_many::<(&mut Health, &Position), 1>([survivor])
            .unwrap();
        health.0 += 1;
        assert_eq!(*position, Position(2.0, 3.0));
        drop((health, position));

        // Sequential despawns push first occurrences onto the free-slot stack.
        for old in [empty, first, last] {
            let replacement = world.spawn(());
            assert_eq!(replacement.id(), old.id());
            assert_eq!(replacement.generation(), old.generation() + 1);
            assert!(!world.has::<RecordDrop>(replacement));
            assert!(!world.has::<Health>(replacement));
            assert!(!world.has::<Velocity>(replacement));
            assert!(matches!(
                world.get_many::<Entity, 1>([old]),
                Err(Error::NoSuchEntity(entity)) if entity == old
            ));
        }
        assert_eq!(world.get::<&Health>(survivor).unwrap().0, 21);
        drop(world);
        assert_eq!(*drops.borrow(), [3, 1]);
    }

    #[test]
    fn despawn_many_validation_failure_preserves_components_generations_and_free_slots() {
        struct CountDrop(Rc<Cell<usize>>);
        impl Drop for CountDrop {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut world = World::new();
        let first = world.spawn((Health(10), CountDrop(drops.clone())));
        let second = world.spawn((Health(20), CountDrop(drops.clone())));
        let stale = world.spawn(());
        world.despawn(stale).unwrap();
        let invalid = Entity::from_bits(u64::MAX);
        let wrong_generation = Entity {
            index: first.id(),
            generation: first.generation() + 1,
        };
        let slots_before: Vec<_> = world
            .slots
            .iter()
            .map(|slot| (slot.alive, slot.generation))
            .collect();
        let free_before = world.free.clone();

        for (batch, expected) in [
            ([first, second, first, stale, invalid], stale),
            ([first, second, first, invalid, stale], invalid),
            ([stale, first, second, stale, invalid], stale),
            (
                [first, second, first, wrong_generation, stale],
                wrong_generation,
            ),
        ] {
            assert_eq!(
                world.despawn_many(&batch),
                Err(Error::NoSuchEntity(expected))
            );
            assert_eq!(world.len(), 2);
            assert_eq!(world.entities().collect::<Vec<_>>(), [first, second]);
            assert_eq!(drops.get(), 0);
            assert_eq!(world.free, free_before);
            assert_eq!(
                world
                    .slots
                    .iter()
                    .map(|slot| (slot.alive, slot.generation))
                    .collect::<Vec<_>>(),
                slots_before
            );
            let [a, b] = world.get_many::<&mut Health, 2>([first, second]).unwrap();
            assert_eq!((a.0, b.0), (10, 20));
        }
        let replacement = world.spawn(());
        assert_eq!(replacement.id(), stale.id());
        assert_eq!(replacement.generation(), stale.generation() + 1);
        assert_eq!(world.despawn_many(&[first, second, first]), Ok(2));
        assert_eq!(drops.get(), 2);
        assert!(world.contains(replacement));
    }

    #[test]
    fn despawn_many_rejects_stale_handles_even_when_the_replacement_is_in_the_batch() {
        let mut world = World::new();
        let stale = world.spawn((Health(10),));
        world.despawn(stale).unwrap();
        let replacement = world.spawn((Health(20),));
        let other = world.spawn((Health(30),));
        assert_eq!(stale.id(), replacement.id());
        for batch in [
            [replacement, other, stale],
            [other, stale, replacement],
            [stale, replacement, other],
        ] {
            assert_eq!(world.despawn_many(&batch), Err(Error::NoSuchEntity(stale)));
            assert_eq!(world.len(), 2);
            let [a, b] = world
                .get_many::<&mut Health, 2>([replacement, other])
                .unwrap();
            assert_eq!((a.0, b.0), (20, 30));
        }
        assert_eq!(world.despawn_many(&[replacement, other]), Ok(2));
        assert!(world.is_empty());
    }

    #[test]
    fn despawn_many_empty_batches_are_noops_and_componentless_entities_are_removed() {
        let mut world = World::new();
        assert_eq!(world.despawn_many(&[]), Ok(0));
        let entity = world.spawn(());
        assert_eq!(world.despawn_many(&[]), Ok(0));
        assert!(world.contains(entity));
        assert_eq!(world.len(), 1);
        assert_eq!(world.despawn_many(&[entity, entity]), Ok(1));
        assert!(world.is_empty());
        assert_eq!(world.despawn_many(&[]), Ok(0));
        assert_eq!(
            world.despawn_many(&[entity, entity]),
            Err(Error::NoSuchEntity(entity))
        );
    }

    #[test]
    fn despawn_many_retires_exhausted_generations_without_recycling_duplicates() {
        let mut world = World::new();
        let first = world.spawn((Health(10),));
        world.slots[first.id() as usize].generation = u32::MAX;
        let exhausted = Entity {
            index: first.id(),
            generation: u32::MAX,
        };
        let ordinary = world.spawn((Health(20),));
        assert_eq!(
            world.despawn_many(&[exhausted, ordinary, exhausted, ordinary]),
            Ok(2)
        );
        assert!(world.is_empty());
        let reused = world.spawn(());
        assert_eq!(reused.id(), ordinary.id());
        assert_eq!(reused.generation(), ordinary.generation() + 1);
        let fresh = world.spawn(());
        assert_ne!(fresh.id(), exhausted.id());
        assert_ne!(fresh.id(), reused.id());
        assert!(!world.contains(first));
        assert!(!world.contains(exhausted));
        assert!(!world.contains(ordinary));
    }

    #[test]
    fn despawn_many_destructor_panic_preserves_later_entities_and_retires_current_slot() {
        struct PanicOnDrop;
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("component destructor failed");
            }
        }
        let mut world = World::new();
        let before = world.spawn((Health(10),));
        let panicking = world.spawn((PanicOnDrop, Health(20)));
        let after = world.spawn((Health(30),));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.despawn_many(&[before, panicking, after]).unwrap();
        }));
        assert!(result.is_err());
        assert!(!world.contains(before));
        assert!(!world.contains(panicking));
        assert_eq!(world.entities().collect::<Vec<_>>(), [after]);
        assert_eq!(world.len(), 1);
        assert_eq!(world.get::<&Health>(after).unwrap().0, 30);
        let reused = world.spawn(());
        assert_eq!(reused.id(), before.id());
        let fresh = world.spawn(());
        assert_ne!(fresh.id(), panicking.id());
        assert!(!world.has::<Health>(reused));
        assert!(!world.has::<Health>(fresh));
        assert_eq!(world.despawn_many(&[after, reused, fresh]), Ok(3));
        assert!(world.is_empty());
    }

    #[test]
    fn tuple_queries_update_only_matching_entities_and_return_ids() {
        let mut world = World::new();
        let matching = world.spawn((Position(2.0, 3.0), Velocity(4.0, -1.0), Health(30), "pilot"));
        let stationary = world.spawn((Position(20.0, 30.0), Health(100)));
        let removed = world.spawn((Position(9.0, 9.0), Velocity(1.0, 1.0)));
        world.despawn(removed).unwrap();
        for (entity, mut position, mut velocity) in
            &mut world.query::<(Entity, &mut Position, &mut Velocity)>()
        {
            assert_eq!(entity, matching);
            position.0 += velocity.0;
            position.1 += velocity.1;
            velocity.0 *= 0.5;
        }
        assert_eq!(
            *world.get::<&Position>(matching).unwrap(),
            Position(6.0, 2.0)
        );
        assert_eq!(
            *world.get::<&Velocity>(matching).unwrap(),
            Velocity(2.0, -1.0)
        );
        assert_eq!(
            *world.get::<&Position>(stationary).unwrap(),
            Position(20.0, 30.0)
        );
        assert_eq!(
            world
                .query::<(&Position, &Velocity, &Health, &&str)>()
                .iter()
                .count(),
            1
        );
        assert_eq!(world.query::<&Position>().iter().count(), 2);
        assert_eq!(
            world.entities().collect::<Vec<_>>(),
            vec![matching, stationary]
        );
    }

    #[test]
    fn guards_can_outlive_iterators_and_other_entity_guards() {
        let mut world = World::new();
        let a = world.spawn((Health(1),));
        let b = world.spawn((Health(2),));
        let (mut first, mut second) = {
            let mut items = world.query::<&mut Health>().iter();
            let first = items.next().unwrap();
            let second = items.next().unwrap();
            assert!(items.next().is_none());
            (first, second)
        };
        first.0 += 10;
        second.0 += 20;
        assert!(matches!(
            world.get::<&Health>(a),
            Err(Error::BorrowConflict(_))
        ));
        drop(first);
        assert_eq!(world.get::<&Health>(a).unwrap().0, 11);
        assert!(matches!(
            world.get::<&Health>(b),
            Err(Error::BorrowConflict(_))
        ));
        drop(second);
        assert_eq!(world.get::<&Health>(b).unwrap().0, 22);
    }

    #[test]
    fn borrow_conflicts_are_fallible_and_release_on_drop() {
        let mut world = World::new();
        let entity = world.spawn((Health(7), Position(2.0, 3.0)));
        let shared = world.get::<&Health>(entity).unwrap();
        let other_shared = world.get::<&Health>(entity).unwrap();
        assert!(matches!(
            world.get::<&mut Health>(entity),
            Err(Error::BorrowConflict(_))
        ));
        assert!(world.get_mut::<Position>(entity).is_ok());
        drop((shared, other_shared));
        let mut exclusive = world.get_mut::<Health>(entity).unwrap();
        exclusive.0 = 9;
        assert!(matches!(
            world.get::<&Health>(entity),
            Err(Error::BorrowConflict(_))
        ));
        assert!(matches!(
            world.get_mut::<Health>(entity),
            Err(Error::BorrowConflict(_))
        ));
        drop(exclusive);
        assert_eq!(world.get::<&Health>(entity).unwrap().0, 9);
    }

    #[test]
    fn get_many_borrows_tuples_in_requested_order_and_updates_selected_entities() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Velocity(1.0, 0.0)));
        let untouched = world.spawn((Health(20), Velocity(2.0, 0.0)));
        let last = world.spawn((Health(30), Velocity(3.0, 0.0)));
        let [
            (last_id, mut last_health, last_speed),
            (first_id, mut first_health, first_speed),
        ] = world
            .get_many::<(Entity, &mut Health, &Velocity), 2>([last, first])
            .unwrap();
        assert_eq!((last_id, first_id), (last, first));
        last_health.0 -= last_speed.0 as i32;
        first_health.0 += first_speed.0 as i32;
        assert!(matches!(
            world.get::<&Health>(first),
            Err(Error::BorrowConflict(_))
        ));
        assert_eq!(world.get::<&Health>(untouched).unwrap().0, 20);
        drop((last_health, first_health, last_speed, first_speed));
        assert_eq!(world.get::<&Health>(last).unwrap().0, 27);
        assert_eq!(world.get::<&Health>(first).unwrap().0, 11);
    }

    #[test]
    fn get_many_allows_repeated_shared_access_and_identity_only_queries() {
        let mut world = World::new();
        let entity = world.spawn((Health(10),));
        let [(a, b), (c, d)] = world
            .get_many::<(&Health, &Health), 2>([entity, entity])
            .unwrap();
        assert_eq!((a.0, b.0, c.0, d.0), (10, 10, 10, 10));
        assert!(matches!(
            world.get_mut::<Health>(entity),
            Err(Error::BorrowConflict(_))
        ));
        drop((a, b, c, d));
        let held = world.get_mut::<Health>(entity).unwrap();
        assert_eq!(
            world.get_many::<Entity, 2>([entity, entity]).unwrap(),
            [entity, entity]
        );
        assert_eq!(held.0, 10);
    }

    #[test]
    fn get_many_rejects_duplicate_mutable_access_and_releases_all_prior_guards() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Position(1.0, 2.0)));
        let second = world.spawn((Health(20), Position(3.0, 4.0)));
        assert!(matches!(
            world.get_many::<(&Position, &mut Health), 3>([first, second, first]),
            Err(Error::BorrowConflict(name)) if name == type_name::<Health>()
        ));
        // Both earlier entities and the shared borrow from the partial final tuple
        // must be released when duplicate mutable access fails.
        for entity in [first, second] {
            assert!(world.get_mut::<Position>(entity).is_ok());
            assert!(world.get_mut::<Health>(entity).is_ok());
        }
        assert!(matches!(
            world.get_many::<(&mut Health, &Health), 1>([first]),
            Err(Error::ConflictingQuery(name)) if name == type_name::<Health>()
        ));
        assert!(matches!(
            world.get_many::<(&mut Health, &mut Health), 1>([first]),
            Err(Error::ConflictingQuery(_))
        ));
        assert_eq!(world.get::<&Health>(first).unwrap().0, 10);
        assert_eq!(world.get::<&Health>(second).unwrap().0, 20);
    }

    #[test]
    fn get_many_missing_component_releases_completed_and_partial_tuples() {
        let mut world = World::new();
        let complete = world.spawn((Health(10), Position(1.0, 2.0)));
        let incomplete = world.spawn((Health(20),));
        assert!(matches!(
            world.get_many::<(&mut Health, &mut Position), 2>([complete, incomplete]),
            Err(Error::MissingComponent(name)) if name == type_name::<Position>()
        ));
        assert!(world.get_mut::<Position>(complete).is_ok());
        assert_eq!(world.get_mut::<Health>(complete).unwrap().0, 10);
        assert_eq!(world.get_mut::<Health>(incomplete).unwrap().0, 20);
        world.insert(incomplete, (Position(3.0, 4.0),)).unwrap();
        assert!(
            world
                .get_many::<(&mut Health, &mut Position), 2>([complete, incomplete])
                .is_ok()
        );
    }

    #[test]
    fn get_many_external_conflict_preserves_existing_guard_and_releases_new_ones() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Position(1.0, 2.0)));
        let second = world.spawn((Health(20), Position(3.0, 4.0)));
        let held = world.get::<&Position>(second).unwrap();
        assert!(matches!(
            world.get_many::<(&mut Health, &mut Position), 2>([first, second]),
            Err(Error::BorrowConflict(name)) if name == type_name::<Position>()
        ));
        assert_eq!(*held, Position(3.0, 4.0));
        assert!(world.get_mut::<Position>(first).is_ok());
        assert_eq!(world.get_mut::<Health>(first).unwrap().0, 10);
        assert_eq!(world.get_mut::<Health>(second).unwrap().0, 20);
        assert!(matches!(
            world.get_mut::<Position>(second),
            Err(Error::BorrowConflict(_))
        ));
        drop(held);
        assert!(
            world
                .get_many::<(&mut Health, &mut Position), 2>([first, second])
                .is_ok()
        );
    }

    #[test]
    fn get_many_checks_all_identities_before_borrowing_and_rejects_reused_handles() {
        let mut world = World::new();
        let alive = world.spawn((Health(10),));
        let stale = world.spawn((Health(20),));
        world.despawn(stale).unwrap();
        assert!(matches!(
            world.get_many::<&mut Health, 2>([alive, stale]),
            Err(Error::NoSuchEntity(entity)) if entity == stale
        ));
        let replacement = world.spawn((Health(30),));
        assert_eq!(stale.id(), replacement.id());
        assert_ne!(stale.generation(), replacement.generation());
        let held = world.get_mut::<Health>(alive).unwrap();
        // Invalid identity wins even when an earlier entity is already borrowed.
        for invalid in [stale, Entity::from_bits(u64::MAX)] {
            assert!(matches!(
                world.get_many::<&mut Health, 2>([alive, invalid]),
                Err(Error::NoSuchEntity(entity)) if entity == invalid
            ));
        }
        assert!(matches!(
            world.get_many::<&mut Health, 2>([replacement, stale]),
            Err(Error::NoSuchEntity(entity)) if entity == stale
        ));
        assert!(matches!(
            world.get_many::<Entity, 1>([stale]),
            Err(Error::NoSuchEntity(entity)) if entity == stale
        ));
        assert_eq!(held.0, 10);
        drop(held);
        let [mut health] = world.get_many::<&mut Health, 1>([replacement]).unwrap();
        health.0 += 1;
        drop(health);
        assert_eq!(world.get::<&Health>(replacement).unwrap().0, 31);
    }

    #[test]
    fn get_many_empty_input_still_validates_query_signature() {
        let world = World::new();
        assert!(world.get_many::<&mut Health, 0>([]).unwrap().is_empty());
        assert!(matches!(
            world.get_many::<(&mut Health, &Health), 0>([]),
            Err(Error::ConflictingQuery(_))
        ));
    }

    #[test]
    fn query_aliases_reject_mutable_duplicates_allow_shared_duplicates() {
        let mut world = World::new();
        world.spawn((Health(10),));
        assert!(matches!(
            world.try_query::<(&mut Health, &Health)>(),
            Err(Error::ConflictingQuery(_))
        ));
        assert!(matches!(
            world.try_query::<(&mut Health, &mut Health)>(),
            Err(Error::ConflictingQuery(_))
        ));
        let values = world.query::<(&Health, &Health)>().iter().next().unwrap();
        assert_eq!(values.0.0, values.1.0);
        assert_eq!(world.query::<(Entity, Entity)>().iter().count(), 1);
    }

    #[test]
    fn tuple_borrow_failure_releases_partial_borrows_and_continues() {
        let mut world = World::new();
        let first = world.spawn((Health(1), Position(1.0, 1.0)));
        let second = world.spawn((Health(2), Position(2.0, 2.0)));
        let lock = world.get_mut::<Position>(first).unwrap();
        let mut iterator = world.query::<(&mut Health, &mut Position)>().try_iter();
        assert!(matches!(
            iterator.next().unwrap(),
            Err(Error::BorrowConflict(_))
        ));
        // Fetching Health succeeded before Position failed, so it must have been released.
        assert!(world.get_mut::<Health>(first).is_ok());
        let (mut health, _) = iterator.next().unwrap().unwrap();
        health.0 = 12;
        drop((iterator, health, lock));
        assert_eq!(world.get::<&Health>(second).unwrap().0, 12);
    }

    #[test]
    fn missing_components_are_filtered_before_any_borrow_attempt() {
        let mut world = World::new();
        let entity = world.spawn((Health(10),));
        let guard = world.get_mut::<Health>(entity).unwrap();
        let mut query = world.query::<(&mut Health, &Position)>().try_iter();
        assert!(query.next().is_none());
        drop(guard);
    }

    #[test]
    fn optional_members_retain_required_matches_and_update_present_components() {
        let mut world = World::new();
        let moving = world.spawn((Position(1.0, 2.0), Velocity(3.0, 4.0), Health(10)));
        let stationary = world.spawn((Position(5.0, 6.0),));
        let no_position = world.spawn((Velocity(9.0, 9.0), Health(20)));
        let held = world.get_mut::<Velocity>(no_position).unwrap();
        let mut visited = Vec::new();
        for row in world
            .query::<(
                Entity,
                &mut Position,
                Option<&Velocity>,
                Option<&mut Health>,
            )>()
            .try_iter()
        {
            let (entity, mut position, velocity, health) = row.unwrap();
            visited.push(entity);
            if let Some(velocity) = velocity {
                position.0 += velocity.0;
                position.1 += velocity.1;
            }
            if let Some(mut health) = health {
                health.0 -= 1;
            }
        }
        assert_eq!(visited, [moving, stationary]);
        assert_eq!(*world.get::<&Position>(moving).unwrap(), Position(4.0, 6.0));
        assert_eq!(
            *world.get::<&Position>(stationary).unwrap(),
            Position(5.0, 6.0)
        );
        assert_eq!(world.get::<&Health>(moving).unwrap().0, 9);
        assert_eq!(world.get::<&Health>(no_position).unwrap().0, 20);
        assert!(!world.has::<Health>(stationary));
        assert_eq!(*held, Velocity(9.0, 9.0));
    }

    #[test]
    fn optional_only_queries_visit_every_live_slot_in_order() {
        let mut world = World::new();
        assert!(world.query::<Option<&Health>>().iter().next().is_none());
        let before_storage = world.spawn(());
        let present = world.spawn((Health(10),));
        let dead = world.spawn((Health(20),));
        let beyond_storage = world.spawn(());
        world.despawn(dead).unwrap();

        let rows: Vec<_> = world
            .query::<(Entity, Option<&Health>, Option<&Velocity>)>()
            .iter()
            .map(|(entity, health, velocity)| {
                assert!(velocity.is_none()); // This type has never been registered.
                (entity, health.map(|health| health.0))
            })
            .collect();
        assert_eq!(
            rows,
            [
                (before_storage, None),
                (present, Some(10)),
                (beyond_storage, None)
            ]
        );
        let mut iter = world.query::<Option<&mut Health>>().try_iter();
        assert!(iter.next().unwrap().unwrap().is_none());
        iter.next().unwrap().unwrap().unwrap().0 += 1;
        assert!(iter.next().unwrap().unwrap().is_none());
        assert!(iter.next().is_none());
        assert!(iter.next().is_none());
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert_eq!(world.get::<&Health>(present).unwrap().0, 11);
        assert_eq!(world.query::<Option<&mut Velocity>>().iter().count(), 3);
    }

    #[test]
    fn optional_aliases_are_rejected_even_on_empty_worlds_and_batches() {
        fn rejects<Q: Query>(world: &World, entities: [Entity; 1]) {
            assert!(matches!(world.try_query::<Q>(),
                Err(Error::ConflictingQuery(name)) if name == type_name::<Health>()));
            assert!(matches!(world.get_many::<Q, 0>([]),
                Err(Error::ConflictingQuery(name)) if name == type_name::<Health>()));
            assert!(matches!(world.get_many::<Q, 1>(entities),
                Err(Error::ConflictingQuery(name)) if name == type_name::<Health>()));
        }
        let mut world = World::new();
        // Signature errors precede identity errors, even without any components.
        for entities in [[Entity::from_bits(u64::MAX)], [world.spawn(())]] {
            rejects::<(&Health, Option<&mut Health>)>(&world, entities);
            rejects::<(Option<&mut Health>, &Health)>(&world, entities);
            rejects::<(&mut Health, Option<&Health>)>(&world, entities);
            rejects::<(Option<&Health>, &mut Health)>(&world, entities);
            rejects::<(&mut Health, Option<&mut Health>)>(&world, entities);
            rejects::<(Option<&mut Health>, &mut Health)>(&world, entities);
            rejects::<(Option<&Health>, Option<&mut Health>)>(&world, entities);
            rejects::<(Option<&mut Health>, Option<&Health>)>(&world, entities);
            rejects::<(Option<&mut Health>, Option<&mut Health>)>(&world, entities);
        }
        let empty = World::new();
        rejects::<(Option<&mut Health>, Option<&Health>)>(&empty, [Entity::from_bits(0)]);
    }

    #[test]
    fn optional_get_distinguishes_absence_from_incompatible_borrows() {
        let mut world = World::new();
        let present = world.spawn((Health(10),));
        let absent = world.spawn(());
        let shared = world.get::<Option<&Health>>(present).unwrap().unwrap();
        assert!(matches!(world.get::<Option<&mut Health>>(present),
            Err(Error::BorrowConflict(name)) if name == type_name::<Health>()));
        assert!(world.get::<Option<&mut Health>>(absent).unwrap().is_none());
        assert_eq!(
            world.get::<Option<&Health>>(present).unwrap().unwrap().0,
            10
        );
        drop(shared);
        let mut exclusive = world.get::<Option<&mut Health>>(present).unwrap().unwrap();
        exclusive.0 += 1;
        assert!(matches!(
            world.get::<Option<&Health>>(present),
            Err(Error::BorrowConflict(_))
        ));
        assert!(matches!(
            world.get::<Option<&mut Health>>(present),
            Err(Error::BorrowConflict(_))
        ));
        assert!(world.get::<Option<&Health>>(absent).unwrap().is_none());
        drop(exclusive);
        assert_eq!(
            world.get::<Option<&Health>>(present).unwrap().unwrap().0,
            11
        );
    }

    #[test]
    fn optional_iterator_conflicts_release_partial_guards_and_continue() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Position(1.0, 2.0)));
        let absent = world.spawn(());
        let last = world.spawn((Health(30), Position(3.0, 4.0)));
        let held = world.get_mut::<Position>(first).unwrap();
        let (health, position) = {
            let mut iter = world
                .query::<(Option<&mut Health>, Option<&Position>)>()
                .try_iter();
            assert!(matches!(iter.next().unwrap(),
                Err(Error::BorrowConflict(name)) if name == type_name::<Position>()));
            assert_eq!(world.get_mut::<Health>(first).unwrap().0, 10);
            let (health, position) = iter.next().unwrap().unwrap();
            assert!(health.is_none() && position.is_none());
            assert!(!world.has::<Health>(absent));
            let row = iter.next().unwrap().unwrap();
            assert!(iter.next().is_none());
            row
        };
        // Guards outlive the iterator and conflict with a new one.
        assert!(matches!(
            world
                .query::<Option<&mut Health>>()
                .try_iter()
                .nth(2)
                .unwrap(),
            Err(Error::BorrowConflict(_))
        ));
        health.unwrap().0 += 1;
        assert!(matches!(
            world.get_mut::<Position>(last),
            Err(Error::BorrowConflict(_))
        ));
        drop((position, held));
        assert_eq!(world.get::<&Health>(last).unwrap().0, 31);
        assert_eq!(
            world
                .query::<(Option<&mut Health>, Option<&mut Position>)>()
                .iter()
                .count(),
            3
        );
    }

    #[test]
    fn optional_members_still_prefilter_all_required_components_before_borrowing() {
        let mut world = World::new();
        let entity = world.spawn((Health(10),));
        let held = world.get_mut::<Health>(entity).unwrap();
        assert!(
            world
                .query::<(Option<&Health>, &Position)>()
                .try_iter()
                .next()
                .is_none()
        );
        assert!(
            world
                .query::<(Option<&mut Health>, &Position)>()
                .try_iter()
                .next()
                .is_none()
        );
        drop(held);
        // get_many retains its fetch-order semantics instead of filtering entities.
        assert!(
            matches!(world.get_many::<(Option<&mut Health>, &Position), 1>([entity]),
            Err(Error::MissingComponent(name)) if name == type_name::<Position>())
        );
        assert_eq!(world.get_mut::<Health>(entity).unwrap().0, 10);
    }

    #[test]
    fn optional_get_many_keeps_order_and_allows_repeated_absent_mutable_access() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Position(1.0, 2.0)));
        let absent = world.spawn(());
        let last = world.spawn((Health(30),));
        let rows = world
            .get_many::<(Entity, Option<&mut Health>, Option<&Position>), 4>([
                last, absent, first, absent,
            ])
            .unwrap();
        for ((entity, health, position), expected) in
            rows.into_iter().zip([last, absent, first, absent])
        {
            assert_eq!(entity, expected);
            assert_eq!(health.is_some(), entity != absent);
            assert_eq!(position.is_some(), entity == first);
            if let Some(mut health) = health {
                health.0 += 1;
            }
        }
        assert_eq!(world.get::<&Health>(first).unwrap().0, 11);
        assert_eq!(world.get::<&Health>(last).unwrap().0, 31);
        assert!(
            world
                .get_many::<Option<&mut Health>, 0>([])
                .unwrap()
                .is_empty()
        );
        assert!(
            world
                .get_many::<Option<&mut Health>, 2>([absent, absent])
                .unwrap()
                .iter()
                .all(Option::is_none)
        );
    }

    #[test]
    fn optional_shared_duplicates_work_with_required_access_and_repeated_entities() {
        let mut world = World::new();
        let entity = world.spawn((Health(10),));
        let rows = world
            .get_many::<(Entity, &Health, Option<&Health>, Option<&Health>), 2>([entity, entity])
            .unwrap();
        for (id, required, optional_a, optional_b) in &rows {
            assert_eq!(*id, entity);
            assert_eq!(required.0, 10);
            assert_eq!(optional_a.as_ref().unwrap().0, 10);
            assert_eq!(optional_b.as_ref().unwrap().0, 10);
        }
        assert_eq!(
            world
                .query::<(&Health, Option<&Health>, Option<&Health>)>()
                .iter()
                .count(),
            1
        );
        assert!(matches!(
            world.get_mut::<Health>(entity),
            Err(Error::BorrowConflict(_))
        ));
        drop(rows);
        assert!(world.get_mut::<Health>(entity).is_ok());
    }

    #[test]
    fn optional_get_many_duplicate_mutable_entities_release_completed_and_partial_guards() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Position(1.0, 2.0)));
        let second = world.spawn((Health(20), Position(3.0, 4.0)));
        assert!(
            matches!(world.get_many::<(Option<&Position>, Option<&mut Health>), 3>([first, second, first]),
            Err(Error::BorrowConflict(name)) if name == type_name::<Health>())
        );
        for entity in [first, second] {
            assert!(world.get_mut::<Position>(entity).is_ok());
            assert!(world.get_mut::<Health>(entity).is_ok());
        }
        assert_eq!(world.get::<&Health>(first).unwrap().0, 10);
        assert_eq!(world.get::<&Health>(second).unwrap().0, 20);
        assert!(
            world
                .get_many::<(Option<&Position>, Option<&mut Health>), 2>([first, second])
                .is_ok()
        );
    }

    #[test]
    fn optional_get_many_external_conflict_releases_new_guards_and_preserves_existing_one() {
        let mut world = World::new();
        let first = world.spawn((Health(10), Position(1.0, 2.0)));
        let second = world.spawn((Health(20), Position(3.0, 4.0)));
        let held = world.get::<&Position>(second).unwrap();
        assert!(
            matches!(world.get_many::<(Option<&mut Health>, Option<&mut Position>), 2>([first, second]),
            Err(Error::BorrowConflict(name)) if name == type_name::<Position>())
        );
        assert!(world.get_mut::<Position>(first).is_ok());
        assert_eq!(world.get_mut::<Health>(first).unwrap().0, 10);
        assert_eq!(world.get_mut::<Health>(second).unwrap().0, 20);
        assert_eq!(*held, Position(3.0, 4.0));
        assert!(matches!(
            world.get_mut::<Position>(second),
            Err(Error::BorrowConflict(_))
        ));
        drop(held);
        assert!(
            world
                .get_many::<(Option<&mut Health>, Option<&mut Position>), 2>([first, second])
                .is_ok()
        );

        world.remove::<Position>(second).unwrap();
        assert!(matches!(
            world.get_many::<(Option<&mut Health>, &Position), 2>([first, second]),
            Err(Error::MissingComponent(_))
        ));
        assert!(world.get_mut::<Position>(first).is_ok());
        assert!(world.get_mut::<Health>(first).is_ok());
        assert!(world.get_mut::<Health>(second).is_ok());
    }

    #[test]
    fn optional_access_observes_remove_reinsert_and_reuse_without_accepting_stale_handles() {
        let mut world = World::new();
        let stale = world.spawn((Health(10),));
        assert_eq!(world.remove::<Health>(stale).unwrap().0, 10);
        assert!(world.get::<Option<&Health>>(stale).unwrap().is_none());
        world.insert(stale, (Health(20),)).unwrap();
        assert_eq!(
            world.get::<Option<&mut Health>>(stale).unwrap().unwrap().0,
            20
        );
        world.despawn(stale).unwrap();
        let replacement = world.spawn(());
        assert_eq!(stale.id(), replacement.id());
        assert_ne!(stale.generation(), replacement.generation());
        assert!(world.get::<Option<&Health>>(replacement).unwrap().is_none());
        let alive = world.spawn((Health(30),));
        let held = world.get_mut::<Health>(alive).unwrap();
        for invalid in [stale, Entity::from_bits(u64::MAX)] {
            assert!(matches!(world.get::<Option<&Health>>(invalid),
                Err(Error::NoSuchEntity(entity)) if entity == invalid));
            assert!(matches!(world.get::<Option<&mut Health>>(invalid),
                Err(Error::NoSuchEntity(entity)) if entity == invalid));
            // All identities are checked before the earlier borrowed component.
            assert!(
                matches!(world.get_many::<Option<&mut Health>, 2>([alive, invalid]),
                Err(Error::NoSuchEntity(entity)) if entity == invalid)
            );
        }
        drop(held);
        let ids: Vec<_> = world
            .query::<(Entity, Option<&Health>)>()
            .iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(ids, [replacement, alive]);
        world.despawn_many(&[replacement, alive]).unwrap();
        assert!(world.query::<Option<&mut Health>>().iter().next().is_none());
        let restarted = world.spawn((Health(40),));
        assert_eq!(
            world.get::<Option<&Health>>(restarted).unwrap().unwrap().0,
            40
        );
    }

    #[test]
    fn optional_absence_does_not_register_storage_or_grow_world_buffers() {
        let mut world = World::new();
        let entity = world.spawn(());
        let capacities = (
            world.slots.capacity(),
            world.free.capacity(),
            world.storages.capacity(),
        );
        assert!(world.get::<Option<&Health>>(entity).unwrap().is_none());
        assert!(
            world
                .query::<Option<&mut Health>>()
                .iter()
                .next()
                .unwrap()
                .is_none()
        );
        let rows = world
            .get_many::<Option<&mut Health>, 128>([entity; 128])
            .unwrap();
        assert!(rows.iter().all(Option::is_none));
        drop(rows);
        assert!(world.storages.is_empty());
        assert_eq!(
            capacities,
            (
                world.slots.capacity(),
                world.free.capacity(),
                world.storages.capacity()
            )
        );
        assert_eq!(world.len(), 1);
        world.insert(entity, (Health(10),)).unwrap();
        assert_eq!(world.get::<Option<&Health>>(entity).unwrap().unwrap().0, 10);
    }

    #[test]
    fn optional_panicking_iteration_releases_partial_guards() {
        let mut world = World::new();
        let entity = world.spawn((Health(10), Position(1.0, 2.0)));
        let held = world.get::<&Position>(entity).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world
                .query::<(Option<&mut Health>, Option<&mut Position>)>()
                .iter()
                .next();
        }));
        assert!(result.is_err());
        assert_eq!(world.get_mut::<Health>(entity).unwrap().0, 10);
        assert_eq!(*held, Position(1.0, 2.0));
        drop(held);
        assert!(world.get_mut::<Position>(entity).is_ok());
    }

    #[test]
    fn duplicate_bundles_are_rejected_before_mutation() {
        let mut world = World::new();
        assert!(matches!(
            world.try_spawn((1_i32, 2_i32)),
            Err(Error::DuplicateComponent(_))
        ));
        assert!(world.is_empty());
        let entity = world.spawn((Health(10), 5_i32));
        assert!(matches!(
            world.insert(entity, (6_i32, 7_i32)),
            Err(Error::DuplicateComponent(_))
        ));
        assert_eq!(*world.get::<&i32>(entity).unwrap(), 5);
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn insert_replace_remove_and_empty_entities_work() {
        let mut world = World::new();
        let entity = world.spawn(());
        assert!(world.contains(entity));
        world
            .insert(entity, (Health(10), Position(1.0, 2.0)))
            .unwrap();
        world.insert(entity, (Health(25),)).unwrap();
        assert_eq!(world.remove::<Health>(entity).unwrap(), Health(25));
        assert!(matches!(
            world.remove::<Health>(entity),
            Err(Error::MissingComponent(_))
        ));
        assert!(matches!(
            world.get::<&Health>(entity),
            Err(Error::MissingComponent(_))
        ));
        assert_eq!(world.query::<&Position>().iter().count(), 1);
        world.despawn(entity).unwrap();
        assert!(world.is_empty());
    }

    #[test]
    fn components_drop_exactly_once_when_replaced_removed_or_despawned() {
        struct CountDrop(Rc<Cell<usize>>);
        impl Drop for CountDrop {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut world = World::new();
        let entity = world.spawn((CountDrop(drops.clone()),));
        world.insert(entity, (CountDrop(drops.clone()),)).unwrap();
        assert_eq!(drops.get(), 1);
        let removed = world.remove::<CountDrop>(entity).unwrap();
        assert_eq!(drops.get(), 1);
        drop(removed);
        assert_eq!(drops.get(), 2);
        world.insert(entity, (CountDrop(drops.clone()),)).unwrap();
        world.despawn(entity).unwrap();
        assert_eq!(drops.get(), 3);
        drop(world);
        assert_eq!(drops.get(), 3);
    }

    #[test]
    fn independently_overlapping_iterators_conflict_without_aliasing() {
        let mut world = World::new();
        world.spawn((Health(10),));
        let mut first = world.query::<&mut Health>().iter();
        let guard = first.next().unwrap();
        let mut second = world.query::<&mut Health>().try_iter();
        assert!(matches!(
            second.next().unwrap(),
            Err(Error::BorrowConflict(_))
        ));
        drop(guard);
        assert_eq!(world.query::<&Health>().iter().next().unwrap().0, 10);
    }

    #[test]
    fn failed_panicking_query_does_not_poison_component_borrows() {
        let mut world = World::new();
        let entity = world.spawn((Health(10), Position(1.0, 2.0)));
        let lock = world.get_mut::<Position>(entity).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world.query::<(&mut Health, &mut Position)>().iter().next();
        }));
        assert!(result.is_err());
        assert!(world.get_mut::<Health>(entity).is_ok());
        drop(lock);
        assert!(world.get_mut::<Position>(entity).is_ok());
    }

    #[test]
    fn thousands_of_reuse_cycles_preserve_counts_and_components() {
        let mut world = World::new();
        let mut stale = Vec::new();
        for i in 0_u32..2_000 {
            let entity = world.spawn((i,));
            assert_eq!(world.len(), 1);
            assert_eq!(*world.get::<&u32>(entity).unwrap(), i);
            assert!(stale.iter().all(|old| !world.contains(*old)));
            world.despawn(entity).unwrap();
            stale.push(entity);
            assert!(world.is_empty());
        }
        assert_eq!(world.slots.len(), 1);
    }
    #[test]
    fn panicking_destructor_does_not_recycle_partially_cleaned_slot() {
        struct PanicOnDrop;
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("component destructor failed");
            }
        }
        let mut world = World::new();
        let entity = world.spawn((PanicOnDrop, Health(10)));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.despawn(entity).unwrap();
        }));
        assert!(result.is_err());
        assert!(!world.contains(entity));
        assert!(world.is_empty());
        let next = world.spawn((Position(1.0, 2.0),));
        assert_ne!(next.id(), entity.id());
        assert!(!world.has::<Health>(next));
    }
}

#[cfg(test)]
mod journey400_213 {
    use super::*;
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    const SENTINEL: Entity = Entity {
        index: u32::MAX,
        generation: u32::MAX,
    };

    #[derive(Debug)]
    struct Part<const I: usize>(i32);

    type Eight = (
        Part<0>,
        Part<1>,
        Part<2>,
        Part<3>,
        Part<4>,
        Part<5>,
        Part<6>,
        Part<7>,
    );

    fn eight(value: i32) -> Eight {
        (
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
            Part(value),
        )
    }

    #[test]
    fn preparation_is_monotonic_preserves_values_and_is_independent() {
        let mut world = World::default();
        assert_eq!(world.prepared_entity_capacity(), 0);
        assert_eq!(world.prepared_component_capacity::<String>(), 0);
        let live = world.spawn((String::from("retained"),));
        let dead = world.spawn(());
        world.despawn(dead).unwrap();
        let free = world.free.clone();
        world.prepare_entities(12).unwrap();
        world.prepare_entities(2).unwrap();
        assert_eq!(world.prepared_entity_capacity(), 12);
        assert_eq!(world.prepared_component_capacity::<String>(), 0);
        world.prepare_component::<String>(20).unwrap();
        world.prepare_component::<String>(1).unwrap();
        world.prepare_component::<bool>(0).unwrap();
        assert_eq!(world.prepared_component_capacity::<String>(), 20);
        assert_eq!(world.prepared_component_capacity::<bool>(), 0);
        assert_eq!(world.prepared_entity_capacity(), 12);
        assert_eq!(world.free, free);
        assert_eq!(world.slots.len(), 2);
        assert_eq!(world.len(), 1);
        assert_eq!(&*world.get::<&String>(live).unwrap(), "retained");
        assert!(
            world.storages[&TypeId::of::<String>()]
                .as_any()
                .downcast_ref::<Storage<String>>()
                .unwrap()
                .slots[1..]
                .iter()
                .all(Option::is_none)
        );
        assert!(world.slots.capacity() >= 12);
        assert!(world.free.capacity() >= 12);
    }

    #[test]
    fn nonclone_inputs_survive_every_capacity_error_and_can_be_retried() {
        #[derive(Debug)]
        struct Owned {
            value: String,
            drops: Rc<Cell<usize>>,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let inputs = std::array::from_fn::<_, 2, _>(|i| {
            (Owned {
                value: format!("projectile {i}"),
                drops: drops.clone(),
            },)
        });
        let mut world = World::new();
        let mut output = [SENTINEL; 3];
        let (error, inputs) = world.try_spawn_batch(inputs, &mut output[..1]).unwrap_err();
        assert_eq!(
            error,
            SpawnError::OutputTooSmall {
                required: 2,
                available: 1
            }
        );
        let (error, inputs) = world.try_spawn_batch(inputs, &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 2,
                prepared: 0
            }
        );
        world.prepare_entities(2).unwrap();
        let (error, inputs) = world.try_spawn_batch(inputs, &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<Owned>(),
                required: 2,
                prepared: 0,
            }
        );
        assert!(world.storages.is_empty());
        world.prepare_component::<Owned>(1).unwrap();
        let (error, inputs) = world.try_spawn_batch(inputs, &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<Owned>(),
                required: 2,
                prepared: 1,
            }
        );
        assert!(world.is_empty());
        assert!(world.slots.is_empty());
        assert!(world.free.is_empty());
        assert_eq!(output, [SENTINEL; 3]);
        assert_eq!(drops.get(), 0);
        assert_eq!(inputs[0].0.value, "projectile 0");
        assert_eq!(inputs[1].0.value, "projectile 1");
        world.prepare_component::<Owned>(2).unwrap();
        assert_eq!(world.try_spawn_batch(inputs, &mut output).unwrap(), 2);
        assert_eq!(output[2], SENTINEL);
        assert_eq!(
            world.get::<&Owned>(output[1]).unwrap().value,
            "projectile 1"
        );
        world.despawn_many(&output[..2]).unwrap();
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn late_duplicate_precedes_all_other_errors_even_for_empty_batches() {
        type Duplicate = (u8, u16, u32, u64, i8, i16, i32, u8);
        let mut world = World::new();
        let live = world.spawn((9_u8,));
        let dead = world.spawn(());
        world.despawn(dead).unwrap();
        let free = world.free.clone();
        let capacities = (
            world.slots.capacity(),
            world.free.capacity(),
            world.storages.capacity(),
        );
        let inputs: [Duplicate; 2] = [(1, 2, 3, 4, 5, 6, 7, 8), (11, 12, 13, 14, 15, 16, 17, 18)];
        let (error, returned) = world.try_spawn_batch(inputs, &mut []).unwrap_err();
        assert_eq!(error, SpawnError::DuplicateComponent(type_name::<u8>()));
        assert_eq!(returned, inputs);
        let mut output = [SENTINEL; 1];
        let (error, _) = world
            .try_spawn_batch::<Duplicate, 0>([], &mut output)
            .unwrap_err();
        assert_eq!(error, SpawnError::DuplicateComponent(type_name::<u8>()));
        assert_eq!(output, [SENTINEL]);
        assert_eq!(world.free, free);
        assert_eq!(world.len(), 1);
        assert_eq!(world.slots.len(), 2);
        assert_eq!(world.storages.len(), 1);
        assert_eq!(*world.get::<&u8>(live).unwrap(), 9);
        assert_eq!(
            (
                world.slots.capacity(),
                world.free.capacity(),
                world.storages.capacity()
            ),
            capacities
        );
    }

    #[test]
    fn every_component_budget_is_checked_before_any_spawn() {
        for missing in 0..8 {
            let mut world = World::new();
            world.prepare_entities(2).unwrap();
            macro_rules! prepare {
                ($($i:literal),+) => { $(
                    world.prepare_component::<Part<$i>>(if missing == $i { 1 } else { 2 }).unwrap();
                )+ };
            }
            prepare!(0, 1, 2, 3, 4, 5, 6, 7);
            let names = <Eight as sealed::Bundle>::component_types_inline();
            let mut output = [SENTINEL; 2];
            let (error, input) = world
                .try_spawn_batch([eight(10), eight(20)], &mut output)
                .unwrap_err();
            assert_eq!(
                error,
                SpawnError::ComponentCapacity {
                    component: names[missing].unwrap().1,
                    required: 2,
                    prepared: 1,
                }
            );
            assert_eq!((input[0].0.0, input[1].7.0), (10, 20));
            assert_eq!(output, [SENTINEL; 2]);
            assert_eq!(world.len(), 0);
            assert!(world.slots.is_empty());
            macro_rules! empty {
                ($($i:literal),+) => { $(
                    assert!(world.storages[&TypeId::of::<Part<$i>>()].as_any()
                        .downcast_ref::<Storage<Part<$i>>>().unwrap().slots.iter().all(Option::is_none));
                )+ };
            }
            empty!(0, 1, 2, 3, 4, 5, 6, 7);
        }
    }

    #[test]
    fn mixed_free_and_new_slots_preserve_caller_order_and_generations() {
        let mut world = World::new();
        let initial = std::array::from_fn::<_, 4, _>(|i| world.spawn((i as i32,)));
        world.despawn_many(&[initial[1], initial[3]]).unwrap();
        world.prepare_entities(5).unwrap();
        world.prepare_component::<i32>(5).unwrap();
        let mut output = [SENTINEL; 5];
        assert_eq!(
            world.try_spawn_batch([(30_i32,), (10,), (40,)], &mut output),
            Ok(3)
        );
        assert_eq!(output.map(Entity::id), [3, 1, 4, u32::MAX, u32::MAX]);
        assert_eq!(
            output[..3]
                .iter()
                .map(|id| id.generation())
                .collect::<Vec<_>>(),
            [1, 1, 0]
        );
        assert!(!world.contains(initial[1]));
        assert!(!world.contains(initial[3]));
        for (id, expected) in output[..3].iter().zip([30, 10, 40]) {
            assert_eq!(*world.get::<&i32>(*id).unwrap(), expected);
        }
        assert_eq!(
            world.query::<&i32>().iter().map(|v| *v).collect::<Vec<_>>(),
            [0, 10, 2, 30, 40]
        );
        assert_eq!(output[3..], [SENTINEL; 2]);
    }

    #[test]
    fn rejection_does_not_consume_an_available_free_prefix() {
        let mut world = World::new();
        world.prepare_entities(2).unwrap();
        world.prepare_component::<i32>(2).unwrap();
        let live = world.spawn((7_i32,));
        let freed = world.spawn((8_i32,));
        world.despawn(freed).unwrap();
        let mut output = [SENTINEL; 3];
        let input = [(10_i32,), (20,)];
        let (error, input) = world.try_spawn_batch(input, &mut output[..1]).unwrap_err();
        assert_eq!(
            error,
            SpawnError::OutputTooSmall {
                required: 2,
                available: 1
            }
        );
        let (error, input) = world.try_spawn_batch(input, &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 3,
                prepared: 2
            }
        );
        world.prepare_entities(3).unwrap();
        let (error, input) = world.try_spawn_batch(input, &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<i32>(),
                required: 3,
                prepared: 2,
            }
        );
        assert_eq!(output, [SENTINEL; 3]);
        assert_eq!(world.free, [freed.id()]);
        assert_eq!(world.slots.len(), 2);
        assert_eq!(world.len(), 1);
        assert_eq!(*world.get::<&i32>(live).unwrap(), 7);
        world.prepare_component::<i32>(3).unwrap();
        assert_eq!(world.try_spawn_batch(input, &mut output), Ok(2));
        assert_eq!(output.map(Entity::id), [freed.id(), 2, u32::MAX]);
        assert_eq!(output[0].generation(), freed.generation() + 1);
    }

    #[test]
    fn sparse_budgets_follow_selected_lifo_indices_not_live_count() {
        let mut world = World::new();
        let ids = std::array::from_fn::<_, 6, _>(|_| world.spawn(()));
        world.despawn_many(&[ids[5], ids[0]]).unwrap(); // Next selected index is 0.
        world.prepare_entities(1).unwrap();
        world.prepare_component::<i32>(1).unwrap();
        let mut output = [SENTINEL];
        assert_eq!(world.try_spawn_batch([(10_i32,)], &mut output), Ok(1));
        assert_eq!(output[0].id(), 0);
        let previous = output;
        let (error, _) = world.try_spawn_batch([(20_i32,)], &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 6,
                prepared: 1
            }
        );
        assert_eq!(output, previous);
        assert_eq!(world.free, [5]);
        world.prepare_entities(6).unwrap();
        let (error, input) = world.try_spawn_batch([(20_i32,)], &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<i32>(),
                required: 6,
                prepared: 1,
            }
        );
        world.prepare_component::<i32>(6).unwrap();
        assert_eq!(world.try_spawn_batch(input, &mut output), Ok(1));
        assert_eq!(output[0].id(), 5);
        assert_eq!(world.len(), 6);
    }

    #[test]
    fn unprepared_ordinary_growth_and_allocator_rounding_do_not_grant_budget() {
        let mut world = World::new();
        world.prepare_entities(1).unwrap();
        world.prepare_component::<i32>(1).unwrap();
        let first = world.spawn((1_i32,));
        let second = world.spawn((2_i32,)); // Ordinary spawn still grows.
        let mut output = [SENTINEL];
        let (error, _) = world.try_spawn_batch([(3_i32,)], &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 3,
                prepared: 1
            }
        );
        world.prepare_entities(3).unwrap();
        let (error, _) = world.try_spawn_batch([(3_i32,)], &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<i32>(),
                required: 3,
                prepared: 1,
            }
        );
        assert_eq!(*world.get::<&i32>(first).unwrap(), 1);
        assert_eq!(*world.get::<&i32>(second).unwrap(), 2);
        assert_eq!(world.len(), 2);
        assert_eq!(world.prepared_component_capacity::<i32>(), 1);
        assert_eq!(output, [SENTINEL]);
    }

    #[test]
    fn empty_batches_need_no_preparation_and_do_not_register_types() {
        let mut world = World::new();
        let mut output = [SENTINEL; 2];
        assert_eq!(
            world.try_spawn_batch::<(String,), 0>([], &mut output),
            Ok(0)
        );
        assert_eq!(world.try_spawn_batch::<(), 0>([], &mut []), Ok(0));
        assert_eq!(output, [SENTINEL; 2]);
        assert!(world.storages.is_empty());
        assert_eq!(world.slots.capacity(), 0);
        assert_eq!(world.free.capacity(), 0);
        world.prepare_entities(2).unwrap();
        assert_eq!(world.try_spawn_batch([(), ()], &mut output), Ok(2));
        assert_eq!(world.len(), 2);
        assert!(world.storages.is_empty());
        world.despawn_many(&output).unwrap();
        assert_eq!(world.try_spawn_batch([(), ()], &mut output), Ok(2));
        assert_eq!(output.map(Entity::id), [1, 0]);
    }

    #[test]
    fn identity_arithmetic_preflights_without_allocating_huge_worlds() {
        assert_eq!(required_batch_slots(4, &[0, 3], 0), Ok(0));
        assert_eq!(required_batch_slots(4, &[0, 3], 1), Ok(4));
        assert_eq!(required_batch_slots(4, &[0, 3], 3), Ok(5));
        assert_eq!(
            required_batch_slots(usize::MAX, &[], 1),
            Err(SpawnError::IdentityCapacity)
        );
        assert_eq!(
            required_batch_slots(0, &[], usize::MAX),
            if usize::BITS > 32 {
                Err(SpawnError::IdentityCapacity)
            } else {
                Ok(usize::MAX)
            }
        );
        #[cfg(target_pointer_width = "64")]
        {
            let max = u32::MAX as usize + 1;
            assert_eq!(check_identity_capacity(max), Ok(()));
            assert_eq!(required_batch_slots(max - 1, &[], 1), Ok(max));
            assert_eq!(required_batch_slots(max, &[0], 1), Ok(1));
            assert_eq!(
                required_batch_slots(max, &[], 1),
                Err(SpawnError::IdentityCapacity)
            );
            let mut world = World::new();
            world.prepare_entities(2).unwrap();
            world.prepare_component::<i32>(2).unwrap();
            let live = world.spawn((5_i32,));
            assert_eq!(
                world.prepare_entities(max + 1),
                Err(SpawnError::IdentityCapacity)
            );
            assert_eq!(
                world.prepare_component::<i32>(max + 1),
                Err(SpawnError::IdentityCapacity)
            );
            assert_eq!(
                world.prepare_component::<bool>(max + 1),
                Err(SpawnError::IdentityCapacity)
            );
            assert_eq!(world.prepared_entity_capacity(), 2);
            assert_eq!(world.prepared_component_capacity::<i32>(), 2);
            assert_eq!(world.storages.len(), 1);
            assert_eq!(*world.get::<&i32>(live).unwrap(), 5);
        }
    }

    #[test]
    fn reservation_overflow_returns_an_error_without_contents_or_budget_changes() {
        let mut storage = Storage::<u64>::default();
        prepare_storage(&mut storage, 2).unwrap();
        storage.slots[0] = Some(RefCell::new(99));
        assert_eq!(
            prepare_storage(&mut storage, usize::MAX),
            Err(SpawnError::AllocationFailed)
        );
        assert_eq!(storage.prepared, 2);
        assert_eq!(storage.slots.len(), 2);
        assert_eq!(*storage.slots[0].as_ref().unwrap().borrow(), 99);
        // Force Vec layout overflow at a representable identity count. No huge
        // allocation is attempted and no value of this component is constructed.
        #[cfg(target_pointer_width = "64")]
        {
            let mut world = World::new();
            assert_eq!(
                world.prepare_component::<[u8; 1 << 31]>(u32::MAX as usize + 1),
                Err(SpawnError::AllocationFailed)
            );
            assert!(world.storages.is_empty());
            assert!(world.is_empty());
        }
    }

    #[test]
    fn retired_generation_slots_still_consume_sparse_budget() {
        let mut world = World::new();
        world.prepare_entities(1).unwrap();
        let initial = world.spawn(());
        world.slots[0].generation = u32::MAX;
        let exhausted = Entity {
            index: 0,
            generation: u32::MAX,
        };
        world.despawn(exhausted).unwrap();
        assert!(world.is_empty());
        assert!(world.free.is_empty());
        let mut output = [SENTINEL];
        let (error, input) = world.try_spawn_batch([()], &mut output).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 2,
                prepared: 1
            }
        );
        assert_eq!(output, [SENTINEL]);
        world.prepare_entities(2).unwrap();
        assert_eq!(world.try_spawn_batch(input, &mut output), Ok(1));
        assert_eq!((output[0].id(), output[0].generation()), (1, 0));
        assert!(!world.contains(initial));
        assert!(!world.contains(exhausted));
    }

    #[test]
    fn prepared_cold_spawn_and_repeated_resets_keep_all_ecs_buffers() {
        let mut world = World::new();
        world.prepare_entities(16).unwrap();
        macro_rules! prepare {
            ($($i:literal),+) => { $(world.prepare_component::<Part<$i>>(16).unwrap();)+ };
        }
        prepare!(0, 1, 2, 3, 4, 5, 6, 7);
        fn buffers(world: &World) -> Vec<(usize, usize, usize)> {
            let mut result = vec![
                (world.slots.as_ptr() as usize, world.slots.capacity(), 0),
                (world.free.as_ptr() as usize, world.free.capacity(), 0),
                (0, world.storages.capacity(), world.storages.len()),
            ];
            macro_rules! storage {
                ($($i:literal),+) => { $(
                    let storage = world.storages[&TypeId::of::<Part<$i>>()].as_any()
                        .downcast_ref::<Storage<Part<$i>>>().unwrap();
                    result.push((storage.slots.as_ptr() as usize, storage.slots.capacity(),
                        storage as *const Storage<Part<$i>> as usize));
                )+ };
            }
            storage!(0, 1, 2, 3, 4, 5, 6, 7);
            result
        }
        let prepared = buffers(&world);
        let mut output = [SENTINEL; 12];
        let mut old = None;
        for cycle in 0..64 {
            let inputs = std::array::from_fn::<_, 12, _>(|i| eight(cycle * 100 + i as i32));
            assert_eq!(world.try_spawn_batch(inputs, &mut output).unwrap(), 12);
            if let Some(old) = old {
                assert!(!world.contains(old));
            }
            assert_eq!(world.len(), 12);
            assert_eq!(world.get::<&Part<0>>(output[0]).unwrap().0, cycle * 100);
            assert_eq!(
                world.get::<&Part<7>>(output[11]).unwrap().0,
                cycle * 100 + 11
            );
            assert_eq!(buffers(&world), prepared); // Includes the very first spawn.
            old = Some(output[0]);
            world.despawn_many(&output).unwrap();
            assert!(world.is_empty());
            assert_eq!(world.query::<Option<&Part<7>>>().iter().count(), 0);
            assert_eq!(buffers(&world), prepared);
        }
    }

    #[test]
    fn prepared_spawn_does_not_grow_a_full_storage_map() {
        let mut world = World::new();
        world.prepare_entities(1).unwrap();
        world.prepare_component::<u8>(1).unwrap();
        world.prepare_component::<u16>(1).unwrap();
        world.prepare_component::<u32>(1).unwrap();
        world.storages.shrink_to_fit();
        assert_eq!(world.storages.len(), world.storages.capacity());
        let capacity = world.storages.capacity();
        let mut output = [SENTINEL];
        assert_eq!(
            world.try_spawn_batch([(1_u8, 2_u16, 3_u32)], &mut output),
            Ok(1)
        );
        assert_eq!(world.storages.capacity(), capacity);
        world.despawn(output[0]).unwrap();
        assert_eq!(
            world.try_spawn_batch([(4_u8, 5_u16, 6_u32)], &mut output),
            Ok(1)
        );
        assert_eq!(world.storages.capacity(), capacity);
    }

    #[test]
    fn rejection_returns_panicking_drop_value_without_running_destructor() {
        #[derive(Debug)]
        struct PanicDrop(Rc<Cell<bool>>);
        impl Drop for PanicDrop {
            fn drop(&mut self) {
                self.0.set(true);
                panic!("caller dropped rejected input");
            }
        }
        let dropped = Rc::new(Cell::new(false));
        let mut world = World::new();
        let mut output = [SENTINEL];
        let (error, input) = world
            .try_spawn_batch([(PanicDrop(dropped.clone()),)], &mut output)
            .unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 1,
                prepared: 0
            }
        );
        assert!(!dropped.get());
        assert_eq!(output, [SENTINEL]);
        assert!(catch_unwind(AssertUnwindSafe(|| drop(input))).is_err());
        assert!(dropped.get());
        assert!(world.is_empty());
        assert_eq!(world.spawn(()).id(), 0);
    }
}

#[cfg(test)]
mod journey400_214 {
    use super::*;
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Part<const I: usize>(i32);

    #[derive(Debug)]
    struct Counted {
        value: i32,
        clones: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
    }
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                value: self.value,
                clones: self.clones.clone(),
                drops: self.drops.clone(),
            }
        }
    }
    impl Drop for Counted {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    fn counted(value: i32) -> Counted {
        Counted {
            value,
            clones: Rc::new(Cell::new(0)),
            drops: Rc::new(Cell::new(0)),
        }
    }

    fn buffers<T: 'static>(world: &World) -> [usize; 12] {
        let storage = world.storages[&TypeId::of::<T>()]
            .as_any()
            .downcast_ref::<Storage<T>>()
            .unwrap();
        [
            world.slots.len(),
            world.slots.capacity(),
            world.slots.as_ptr() as usize,
            world.free.len(),
            world.free.capacity(),
            world.free.as_ptr() as usize,
            world.storages.len(),
            world.storages.capacity(),
            storage.slots.len(),
            storage.slots.capacity(),
            storage.slots.as_ptr() as usize,
            std::ptr::from_ref(storage) as usize,
        ]
    }

    #[test]
    fn selected_values_are_independent_and_references_require_explicit_remapping() {
        struct Transient;
        #[derive(Clone)]
        struct Link(Entity);
        let mut world = World::new();
        let first = world.spawn((String::from("actor"), Part::<0>(10), Transient));
        let second = world.spawn((String::from("partner"), Part::<0>(20), Transient));
        world.insert(first, (Link(second), second)).unwrap();
        world.insert(second, (Link(first), first)).unwrap();
        world.prepare_entities(4).unwrap();
        world.prepare_component::<String>(4).unwrap();
        world.prepare_component::<Link>(4).unwrap();
        world.prepare_component::<Entity>(4).unwrap();
        let plan = world
            .plan_duplication::<(String, Link, Entity), 2>([second, first])
            .unwrap();
        let map = plan.try_spawn(&mut world).map_err(|(e, _)| e).unwrap();
        assert_eq!(map.map(|(old, _)| old), [second, first]);
        for (old, new) in map {
            assert_ne!(old, new);
            assert!(![first, second].contains(&new));
            assert!(!world.has::<Transient>(new));
            assert!(!world.has::<Part<0>>(new));
            let original_link = world.get::<&Link>(old).unwrap().0;
            assert_eq!(world.get::<&Link>(new).unwrap().0, original_link);
            assert_eq!(*world.get::<&Entity>(new).unwrap(), original_link);
            let replacement = map.iter().find(|(id, _)| *id == original_link).unwrap().1;
            world.get::<&mut Link>(new).unwrap().0 = replacement;
            world.get::<&mut String>(new).unwrap().push_str(" copy");
        }
        assert_eq!(&*world.get::<&String>(first).unwrap(), "actor");
        assert_eq!(&*world.get::<&String>(map[1].1).unwrap(), "actor copy");
        assert_eq!(world.get::<&Link>(map[0].1).unwrap().0, map[1].1);
        assert_eq!(world.get::<&Link>(second).unwrap().0, first);
        assert_eq!(world.len(), 4);
    }

    #[test]
    fn every_supported_selection_width_including_empty_clones_only_that_bundle() {
        macro_rules! width {
            ($($i:literal),*) => {{
                let mut world = World::new();
                let source = world.spawn(($(Part::<$i>($i),)*));
                world.prepare_entities(2).unwrap();
                $(world.prepare_component::<Part<$i>>(2).unwrap();)*
                let plan = world.plan_duplication::<($(Part<$i>,)*), 1>([source]).unwrap();
                let [(old, new)] = plan.try_spawn(&mut world).unwrap();
                assert_eq!(old, source);
                assert_ne!(old, new);
                $(assert_eq!(*world.get::<&Part<$i>>(new).unwrap(), Part::<$i>($i));)*
                assert_eq!(world.len(), 2);
            }};
        }
        width!();
        width!(0);
        width!(0, 1);
        width!(0, 1, 2);
        width!(0, 1, 2, 3);
        width!(0, 1, 2, 3, 4);
        width!(0, 1, 2, 3, 4, 5);
        width!(0, 1, 2, 3, 4, 5, 6);
        width!(0, 1, 2, 3, 4, 5, 6, 7);
    }

    #[test]
    fn late_missing_component_calls_no_clone_and_releases_all_read_guards() {
        let mut world = World::new();
        let component = counted(10);
        let (clones, drops) = (component.clones.clone(), component.drops.clone());
        let first = world.spawn((component, 1_u8));
        let second = world.spawn((counted(20),));
        let freed = world.spawn(());
        world.despawn(freed).unwrap();
        let before = buffers::<Counted>(&world);
        assert_eq!(
            world
                .plan_duplication::<(Counted, u8), 2>([first, second])
                .err(),
            Some(DuplicationError::Access(Error::MissingComponent(
                type_name::<u8>()
            )))
        );
        assert_eq!(clones.get(), 0);
        assert_eq!(drops.get(), 0);
        assert_eq!(world.len(), 2);
        assert_eq!(buffers::<Counted>(&world), before);
        assert_eq!(world.free, [freed.id()]);
        assert!(world.get_mut::<Counted>(first).is_ok());
        assert!(world.get_mut::<Counted>(second).is_ok());
        assert!(world.get_mut::<u8>(first).is_ok());
        world.insert(second, (2_u8,)).unwrap();
        drop(
            world
                .plan_duplication::<(Counted, u8), 2>([first, second])
                .unwrap(),
        );
        assert_eq!(clones.get(), 1);
        assert_eq!(drops.get(), 1); // Discarded captured clone only.
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn last_source_eighth_borrow_conflict_prevents_all_clones_and_cleans_partial_rows() {
        type Selected = (Counted, u8, u16, u32, u64, i8, i16, i32);
        let mut world = World::new();
        let component = counted(10);
        let clones = component.clones.clone();
        let first = world.spawn((component, 1_u8, 2_u16, 3_u32, 4_u64, 5_i8, 6_i16, 7_i32));
        let second = world.spawn((counted(20), 1_u8, 2_u16, 3_u32, 4_u64, 5_i8, 6_i16, 7_i32));
        let held = world.get_mut::<i32>(second).unwrap();
        let before = buffers::<Counted>(&world);
        assert_eq!(
            world.plan_duplication::<Selected, 2>([first, second]).err(),
            Some(DuplicationError::Access(Error::BorrowConflict(
                type_name::<i32>()
            )))
        );
        assert_eq!(clones.get(), 0);
        assert_eq!(world.len(), 2);
        assert_eq!(buffers::<Counted>(&world), before);
        macro_rules! released {
            ($($t:ty),+) => { $(
                assert!(world.get_mut::<$t>(first).is_ok());
                assert!(world.get_mut::<$t>(second).is_ok());
            )+ };
        }
        released!(Counted, u8, u16, u32, u64, i8, i16);
        assert!(world.get_mut::<i32>(first).is_ok());
        assert_eq!(*held, 7);
        drop(held);
        drop(
            world
                .plan_duplication::<Selected, 2>([first, second])
                .unwrap(),
        );
        assert_eq!(clones.get(), 1);
    }

    #[test]
    fn shared_selected_and_mutable_unselected_guards_allow_capture() {
        let mut world = World::new();
        let source = world.spawn((String::from("actor"), 10_i32));
        let selected = world.get::<&String>(source).unwrap();
        let mut unselected = world.get_mut::<i32>(source).unwrap();
        let plan = world.plan_duplication::<(String,), 1>([source]).unwrap();
        *unselected += 1;
        assert_eq!(&*selected, "actor");
        drop((selected, unselected));
        assert!(world.get_mut::<String>(source).is_ok());
        world.prepare_entities(2).unwrap();
        world.prepare_component::<String>(2).unwrap();
        let [(_, copy)] = plan.try_spawn(&mut world).unwrap();
        assert!(!world.has::<i32>(copy));
        assert_eq!(&*world.get::<&String>(copy).unwrap(), "actor");
    }

    #[test]
    fn validation_precedence_covers_duplicate_types_sources_and_all_identities() {
        let mut world = World::new();
        let source = world.spawn((counted(1),));
        let stale = world.spawn(());
        world.despawn(stale).unwrap();
        let replacement = world.spawn(());
        type Duplicate = (Counted, u8, u16, u32, u64, i8, i16, Counted);
        let duplicate = DuplicationError::Access(Error::DuplicateComponent(type_name::<Counted>()));
        assert_eq!(
            world.plan_duplication::<Duplicate, 0>([]).err(),
            Some(duplicate.clone())
        );
        assert_eq!(
            world.plan_duplication::<Duplicate, 1>([stale]).err(),
            Some(duplicate)
        );
        let held = world.get_mut::<Counted>(source).unwrap();
        for invalid in [stale, Entity::from_bits(u64::MAX)] {
            assert_eq!(
                world
                    .plan_duplication::<(Counted,), 3>([source, source, invalid])
                    .err(),
                Some(DuplicationError::Access(Error::NoSuchEntity(invalid)))
            );
        }
        assert_eq!(
            world
                .plan_duplication::<(Counted,), 2>([source, source])
                .err(),
            Some(DuplicationError::DuplicateSource(source))
        );
        assert_eq!(held.clones.get(), 0);
        assert!(world.contains(replacement));
        assert_eq!(world.len(), 2);
        drop(held);
        // Selecting Entity requires a stored component; identity isn't selected implicitly.
        assert_eq!(
            world.plan_duplication::<(Entity,), 1>([source]).err(),
            Some(DuplicationError::Access(Error::MissingComponent(
                type_name::<Entity>()
            )))
        );
    }

    #[test]
    fn empty_plans_need_no_preparation_or_storage_registration() {
        let world = World::new();
        let plan = world.plan_duplication::<(Counted,), 0>([]).unwrap();
        assert!(world.storages.is_empty());
        let mut target = World::new();
        assert_eq!(plan.try_spawn(&mut target).unwrap(), []);
        assert!(target.is_empty());
        assert!(target.storages.is_empty());
        assert_eq!(target.prepared_entities, 0);
    }

    #[test]
    fn captured_snapshot_survives_source_mutation_despawn_and_reuse() {
        let mut world = World::new();
        let source = world.spawn((String::from("captured"),));
        let plan = world.plan_duplication::<(String,), 1>([source]).unwrap();
        world
            .get::<&mut String>(source)
            .unwrap()
            .push_str(" edited");
        world.remove::<String>(source).unwrap();
        world.despawn(source).unwrap();
        world.prepare_entities(1).unwrap();
        world.prepare_component::<String>(1).unwrap();
        let [(old, new)] = plan.try_spawn(&mut world).unwrap();
        assert_eq!(old, source);
        assert_eq!(new.id(), source.id());
        assert_ne!(old, new);
        assert!(!world.contains(old));
        assert_eq!(&*world.get::<&String>(new).unwrap(), "captured");
    }

    #[test]
    fn a_detached_template_can_spawn_into_a_different_world() {
        let plan = {
            let mut source = World::new();
            let entity = source.spawn((42_i32,));
            source.plan_duplication::<(i32,), 1>([entity]).unwrap()
        };
        let mut target = World::new();
        target.prepare_entities(1).unwrap();
        target.prepare_component::<i32>(1).unwrap();
        let [(old, new)] = plan.try_spawn(&mut target).unwrap();
        assert_eq!(old, new); // Equal bits in different worlds are permitted.
        assert_eq!(*target.get::<&i32>(new).unwrap(), 42);
    }

    #[test]
    fn capacity_failure_returns_the_snapshot_without_drops_reclones_or_free_slot_consumption() {
        let mut world = World::new();
        let component = counted(10);
        let (clones, drops) = (component.clones.clone(), component.drops.clone());
        let first = world.spawn((component,));
        let second = world.spawn((counted(20),));
        let free = world.spawn(());
        world.despawn(free).unwrap();
        let plan = world
            .plan_duplication::<(Counted,), 2>([first, second])
            .unwrap();
        assert_eq!(clones.get(), 1);
        let before = buffers::<Counted>(&world);
        let (error, plan) = plan.try_spawn(&mut world).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 4,
                prepared: 0
            }
        );
        assert_eq!(buffers::<Counted>(&world), before);
        world.prepare_entities(4).unwrap();
        let before = buffers::<Counted>(&world);
        let (error, plan) = plan.try_spawn(&mut world).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<Counted>(),
                required: 4,
                prepared: 0,
            }
        );
        assert_eq!(buffers::<Counted>(&world), before);
        assert_eq!(world.free, [free.id()]);
        assert_eq!(world.len(), 2);
        assert_eq!(clones.get(), 1);
        assert_eq!(drops.get(), 0);
        world.get_mut::<Counted>(first).unwrap().value = 99;
        world.prepare_component::<Counted>(4).unwrap();
        let map = plan.try_spawn(&mut world).unwrap();
        assert_eq!(map.map(|(old, _)| old), [first, second]);
        assert_eq!(map.map(|(_, new)| new.id()), [free.id(), 3]);
        assert_eq!(map[0].1.generation(), free.generation() + 1);
        assert_eq!(world.get::<&Counted>(map[0].1).unwrap().value, 10);
        assert_eq!(world.get::<&Counted>(map[1].1).unwrap().value, 20);
        assert_eq!(clones.get(), 1);
        assert_eq!(drops.get(), 0);
        world.despawn_many(&map.map(|(_, new)| new)).unwrap();
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn last_selected_component_budget_is_required_and_retry_preserves_all_values() {
        type Eight = (u8, u16, u32, u64, i8, i16, i32, i64);
        let mut world = World::new();
        let source = world.spawn((1_u8, 2_u16, 3_u32, 4_u64, 5_i8, 6_i16, 7_i32, 8_i64));
        world.prepare_entities(2).unwrap();
        macro_rules! prepare {
            ($($t:ty),+) => { $(world.prepare_component::<$t>(2).unwrap();)+ };
        }
        prepare!(u8, u16, u32, u64, i8, i16, i32);
        let plan = world.plan_duplication::<Eight, 1>([source]).unwrap();
        let (error, plan) = plan.try_spawn(&mut world).unwrap_err();
        assert_eq!(
            error,
            SpawnError::ComponentCapacity {
                component: type_name::<i64>(),
                required: 2,
                prepared: 0,
            }
        );
        assert_eq!(world.len(), 1);
        world.prepare_component::<i64>(2).unwrap();
        let [(_, copy)] = plan.try_spawn(&mut world).unwrap();
        assert_eq!(*world.get::<&u8>(copy).unwrap(), 1);
        assert_eq!(*world.get::<&i64>(copy).unwrap(), 8);
    }

    #[test]
    fn cold_and_reused_duplication_keep_prepared_ecs_buffers_and_reset_generations() {
        let mut world = World::new();
        let sources = [world.spawn((1_i32,)), world.spawn((2_i32,))];
        world.prepare_entities(4).unwrap();
        world.prepare_component::<i32>(4).unwrap();
        // Compare pointers/capacities while allowing entity/free lengths to change.
        let mut baseline = buffers::<i32>(&world);
        baseline[0] = 0;
        baseline[3] = 0;
        let mut previous = None;
        for _ in 0..64 {
            let plan = world.plan_duplication::<(i32,), 2>(sources).unwrap();
            let map = plan.try_spawn(&mut world).unwrap();
            let copies = map.map(|(_, copy)| copy);
            if let Some(old) = previous {
                for old in old {
                    assert!(!world.contains(old));
                }
            }
            assert_eq!(world.len(), 4);
            for (source, copy) in map {
                assert_eq!(
                    *world.get::<&i32>(source).unwrap(),
                    *world.get::<&i32>(copy).unwrap()
                );
            }
            world.despawn_many(&copies).unwrap();
            assert_eq!(world.len(), 2);
            let mut current = buffers::<i32>(&world);
            current[0] = 0;
            current[3] = 0;
            assert_eq!(current, baseline);
            previous = Some(copies);
        }
        world.despawn_many(&sources).unwrap();
        assert!(world.is_empty());
        let template = world.spawn((3_i32,));
        let plan = world.plan_duplication::<(i32,), 1>([template]).unwrap();
        assert!(plan.try_spawn(&mut world).is_ok());
        assert_eq!(world.prepared_entity_capacity(), 4);
        assert_eq!(world.prepared_component_capacity::<i32>(), 4);
        let fresh = World::new();
        assert_eq!(fresh.prepared_entity_capacity(), 0);
    }

    #[test]
    fn clone_panic_unwinds_completed_clones_and_releases_every_source_guard() {
        #[derive(Debug)]
        struct PanicClone;
        impl Clone for PanicClone {
            fn clone(&self) -> Self {
                panic!("user clone failed");
            }
        }
        let mut world = World::new();
        let component = counted(7);
        let (clones, drops) = (component.clones.clone(), component.drops.clone());
        let source = world.spawn((component, PanicClone));
        let before = buffers::<Counted>(&world);
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _ = world.plan_duplication::<(Counted, PanicClone), 1>([source]);
        }));
        assert!(panic.is_err());
        assert_eq!(clones.get(), 1);
        assert_eq!(drops.get(), 1);
        assert_eq!(buffers::<Counted>(&world), before);
        assert_eq!(world.len(), 1);
        assert_eq!(world.get_mut::<Counted>(source).unwrap().value, 7);
        assert!(world.get_mut::<PanicClone>(source).is_ok());
        drop(world);
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn rejected_plan_retains_panicking_drop_values_until_the_caller_discards_it() {
        #[derive(Debug)]
        struct PanicDrop {
            cloned: bool,
            dropped: Rc<Cell<bool>>,
        }
        impl Clone for PanicDrop {
            fn clone(&self) -> Self {
                Self {
                    cloned: true,
                    dropped: self.dropped.clone(),
                }
            }
        }
        impl Drop for PanicDrop {
            fn drop(&mut self) {
                if self.cloned {
                    self.dropped.set(true);
                    panic!("user drop failed");
                }
            }
        }
        let dropped = Rc::new(Cell::new(false));
        let mut world = World::new();
        let source = world.spawn((PanicDrop {
            cloned: false,
            dropped: dropped.clone(),
        },));
        let plan = world.plan_duplication::<(PanicDrop,), 1>([source]).unwrap();
        let (error, plan) = plan.try_spawn(&mut world).unwrap_err();
        assert_eq!(
            error,
            SpawnError::EntityCapacity {
                required: 2,
                prepared: 0
            }
        );
        assert!(!dropped.get());
        assert_eq!(world.len(), 1);
        let panic = catch_unwind(AssertUnwindSafe(|| drop(plan)));
        assert!(panic.is_err());
        assert!(dropped.get());
        assert!(world.get_mut::<PanicDrop>(source).is_ok());
        assert_eq!(world.len(), 1);
    }
}
