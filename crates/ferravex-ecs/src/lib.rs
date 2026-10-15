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

use std::any::{Any, TypeId, type_name};
use std::cell::RefCell;
pub use std::cell::{Ref, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

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

#[derive(Default)]
struct Slot {
    generation: u32,
    alive: bool,
}

trait ErasedStorage {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn erase(&mut self, index: usize);
}

struct Storage<T> {
    slots: Vec<Option<RefCell<T>>>,
}

impl<T> Default for Storage<T> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<T: 'static> ErasedStorage for Storage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn erase(&mut self, index: usize) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = None;
        }
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
    storages: HashMap<TypeId, Box<dyn ErasedStorage>>,
    len: usize,
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

    pub fn contains(&self, entity: Entity) -> bool {
        self.slots
            .get(entity.index as usize)
            .is_some_and(|slot| slot.alive && slot.generation == entity.generation)
    }

    /// Spawns one entity containing zero to four components.
    /// Panics if a bundle repeats a component type; no entity is allocated in that case.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        self.try_spawn(bundle)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_spawn<B: Bundle>(&mut self, bundle: B) -> Result<Entity, Error> {
        validate_bundle::<B>()?;
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
        Ok(entity)
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
    /// Use get::<&T> or get::<&mut T>; returned guards enforce runtime aliasing.
    pub fn get<A: Access>(&self, entity: Entity) -> Result<A::Item<'_>, Error> {
        self.validate(entity)?;
        A::fetch(self, entity)
    }

    pub fn get_mut<T: 'static>(&self, entity: Entity) -> Result<RefMut<'_, T>, Error> {
        self.get::<&mut T>(entity)
    }

    /// Queries up to four component access types, optionally including Entity.
    /// Repeated shared access is valid; any repeated mutable access is rejected.
    pub fn query<Q: Query>(&self) -> QueryBorrow<'_, Q> {
        self.try_query().unwrap_or_else(|error| panic!("{error}"))
    }

    pub fn try_query<Q: Query>(&self) -> Result<QueryBorrow<'_, Q>, Error> {
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

    fn cell<T: 'static>(&self, index: usize) -> Option<&RefCell<T>> {
        self.storages
            .get(&TypeId::of::<T>())
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

mod sealed {
    pub trait Bundle {}
    pub trait Access {}
    pub trait Query {}
}

/// Tuple of zero to four owned component values accepted by spawn and insert.
/// Implementations are sealed so validation and insertion cannot disagree.
pub trait Bundle: sealed::Bundle {
    #[doc(hidden)]
    fn component_types(types: &mut Vec<(TypeId, &'static str)>);
    #[doc(hidden)]
    fn write(self, world: &mut World, index: usize);
}

impl sealed::Bundle for () {}
impl Bundle for () {
    fn component_types(_: &mut Vec<(TypeId, &'static str)>) {}
    fn write(self, _: &mut World, _: usize) {}
}

macro_rules! bundle {
    ($($name:ident : $index:tt),+) => {
        impl<$($name: 'static),+> sealed::Bundle for ($($name,)+) {}
        impl<$($name: 'static),+> Bundle for ($($name,)+) {
            fn component_types(types: &mut Vec<(TypeId, &'static str)>) {
                $(types.push((TypeId::of::<$name>(), type_name::<$name>()));)+
            }
            fn write(self, world: &mut World, index: usize) {
                $(world.put(index, self.$index);)+
            }
        }
    }
}
bundle!(A: 0);
bundle!(A: 0, B: 1);
bundle!(A: 0, B: 1, C: 2);
bundle!(A: 0, B: 1, C: 2, D: 3);

fn validate_bundle<B: Bundle>() -> Result<(), Error> {
    let mut types = Vec::new();
    B::component_types(&mut types);
    for (index, (id, name)) in types.iter().enumerate() {
        if types[..index].iter().any(|other| other.0 == *id) {
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

/// A single component access (&T or &mut T), or the entity identity itself.
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

/// Query signature: one access type or a tuple containing up to four access types.
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

pub struct QueryBorrow<'world, Q: Query> {
    world: &'world World,
    marker: PhantomData<fn() -> Q>,
}

impl<'world, Q: Query> QueryBorrow<'world, Q> {
    pub fn iter(&self) -> QueryIter<'world, Q> {
        QueryIter {
            inner: self.try_iter(),
        }
    }
    /// Visits all matching entities, reporting borrow conflicts without skipping silently.
    /// A conflict releases any partial tuple borrows and does not stop later entities.
    pub fn try_iter(&self) -> TryQueryIter<'world, Q> {
        TryQueryIter {
            world: self.world,
            cursor: 0,
            marker: PhantomData,
        }
    }
}
impl<'world, Q: Query> IntoIterator for &mut QueryBorrow<'world, Q> {
    type Item = Q::Item<'world>;
    type IntoIter = QueryIter<'world, Q>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'world, Q: Query> IntoIterator for &QueryBorrow<'world, Q> {
    type Item = Q::Item<'world>;
    type IntoIter = QueryIter<'world, Q>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'world, Q: Query> IntoIterator for QueryBorrow<'world, Q> {
    type Item = Q::Item<'world>;
    type IntoIter = QueryIter<'world, Q>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct TryQueryIter<'world, Q: Query> {
    world: &'world World,
    cursor: usize,
    marker: PhantomData<fn() -> Q>,
}
impl<'world, Q: Query> Iterator for TryQueryIter<'world, Q> {
    type Item = Result<Q::Item<'world>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(slot) = self.world.slots.get(self.cursor) {
            let entity = Entity {
                index: self.cursor as u32,
                generation: slot.generation,
            };
            self.cursor += 1;
            // Match the entire tuple before borrowing any member.
            if slot.alive && Q::matches(self.world, entity) {
                return Some(Q::fetch(self.world, entity));
            }
        }
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.world.slots.len().saturating_sub(self.cursor)))
    }
}
impl<Q: Query> std::iter::FusedIterator for TryQueryIter<'_, Q> {}

pub struct QueryIter<'world, Q: Query> {
    inner: TryQueryIter<'world, Q>,
}
impl<'world, Q: Query> Iterator for QueryIter<'world, Q> {
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
impl<Q: Query> std::iter::FusedIterator for QueryIter<'_, Q> {}

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
