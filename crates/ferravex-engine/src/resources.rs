//! Typed singleton engine resources without string keys or unsafe downcasts.
//! At most 256 distinct types. Values are host-owned and may have arbitrary size.
//! This single-threaded store does not impose Send/Sync on resources.
//!
//! Use [`Resources::get_or_insert_with`](crate::resources::Resources::get_or_insert_with) for lazy setup, [`Resources::try_init`](crate::resources::Resources::try_init)
//! when setup can fail, and [`Resources::require`](crate::resources::Resources::require)(crate::resources::Resources::require) for a required dependency.
//! Initializers run only for absent types with available capacity. Type names in
//! errors are Rust diagnostic names, not stable serialization identifiers.
//! Use [`Resources::try_update`](crate::resources::Resources::try_update) to validate a replacement before changing an
//! existing resource; successful updates return ownership of the previous value.
//!
//! For simultaneous access to distinct types, use [`Resources::with2_mut`](crate::resources::Resources::with2_mut) or
//! [`Resources::with3_mut`](crate::resources::Resources::with3_mut). Mixed access is available through
//! [`Resources::with2_mut_ref`](crate::resources::Resources::with2_mut_ref), [`Resources::with3_mut_mut_ref`](crate::resources::Resources::with3_mut_mut_ref) and
//! [`Resources::with3_mut_ref_ref`](crate::resources::Resources::with3_mut_ref_ref); put mutable arguments first. All arguments
//! must have distinct [`TypeId`](std::any::TypeId)s, including the shared arguments. For entirely
//! shared access, ordinary repeated [`Resources::require`](crate::resources::Resources::require) calls suffice.
//! Disjoint access uses scoped ordinary references, never removes entries, and
//! allocates no heap storage of its own. Resource types need not be `Clone`,
//! `Send` or `Sync`. Callback allocations and side effects remain caller-owned.
use std::{
    any::{Any, TypeId, type_name},
    collections::{HashMap, hash_map::Entry},
    fmt,
};

/// A required resource is missing, or a new resource would exceed the type limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceError {
    /// Insert or initialize this type before requesting it.
    Missing { type_name: &'static str },
    /// Remove an unused resource type before initializing a new one.
    Capacity {
        type_name: &'static str,
        capacity: usize,
    },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { type_name } => write!(
                f,
                "required resource `{type_name}` is missing; insert or initialize it before use"
            ),
            Self::Capacity {
                type_name,
                capacity,
            } => write!(
                f,
                "cannot initialize resource `{type_name}`: capacity of {capacity} types reached; remove an unused resource type first"
            ),
        }
    }
}

impl std::error::Error for ResourceError {}

/// A disjoint resource request repeated a type or requested an absent type.
///
/// Indices are zero-based positions in the generic type list and callback
/// arguments. All duplicate checks precede presence checks, even if a repeated
/// type is absent. The first repeated position wins; `first` names its earliest
/// occurrence. Otherwise the first missing position wins. Type aliases count as
/// the same type; distinct newtypes can represent separate roles.
///
/// These errors never invoke the callback or change the store. The existing
/// single-resource [`ResourceError`] remains separate and unchanged.
///
/// ```
/// use ferravex_engine::resources::{ResourceAccessError, Resources};
///
/// type Score = u32;
/// let mut resources = Resources::default();
/// assert_eq!(resources.with2_mut::<u32, Score, ()>(|_, _| panic!("not called")),
///     Err(ResourceAccessError::Duplicate {
///         type_name: std::any::type_name::<u32>(), first: 0, second: 1,
///     }));
/// assert_eq!(resources.with2_mut_ref::<u32, u64, ()>(|_, _| panic!("not called")),
///     Err(ResourceAccessError::Missing {
///         type_name: std::any::type_name::<u32>(), index: 0,
///     }));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAccessError {
    /// A type occurs more than once, regardless of shared/mutable access mode.
    Duplicate {
        type_name: &'static str,
        first: usize,
        second: usize,
    },
    /// Insert or initialize this type before requesting the group.
    Missing {
        type_name: &'static str,
        index: usize,
    },
}

impl fmt::Display for ResourceAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate {
                type_name,
                first,
                second,
            } => write!(
                f,
                "resource `{type_name}` is requested at both argument {first} and {second}; disjoint access requires distinct types"
            ),
            Self::Missing { type_name, index } => write!(
                f,
                "required resource `{type_name}` at argument {index} is missing; insert or initialize it before use"
            ),
        }
    }
}

impl std::error::Error for ResourceAccessError {}

/// A store error or the original error returned by a resource initializer.
///
/// The initializer's error needs no trait bounds to be returned. When it
/// implements [`std::error::Error`] and is `'static`, it is also exposed as the
/// error source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceInitError<E> {
    /// Capacity was exhausted before the initializer ran.
    Resource(ResourceError),
    /// Initialization failed; no resource was inserted.
    Initialization { type_name: &'static str, source: E },
}

impl<E: fmt::Display> fmt::Display for ResourceInitError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(f),
            Self::Initialization { type_name, source } => {
                write!(f, "could not initialize resource `{type_name}`: {source}")
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ResourceInitError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resource(error) => Some(error),
            Self::Initialization { source, .. } => Some(source),
        }
    }
}

/// A missing resource or the original error returned by a checked update.
///
/// The updater's error needs no trait bounds to be returned. When it implements
/// [`std::error::Error`] and is `'static`, it is also exposed as the error source.
/// See [`Resources::try_update`] for the replacement and side-effect guarantees.
///
/// ```
/// use ferravex_engine::resources::{ResourceError, ResourceUpdateError, Resources};
///
/// let mut resources = Resources::default();
/// let result = resources.try_update::<u32, ()>(|_| panic!("missing resource"));
/// assert_eq!(result, Err(ResourceUpdateError::Resource(ResourceError::Missing {
///     type_name: std::any::type_name::<u32>(),
/// })));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceUpdateError<E> {
    /// The resource was missing, so the updater did not run.
    Resource(ResourceError),
    /// The updater failed; the stored resource was not replaced.
    Update { type_name: &'static str, source: E },
}

impl<E: fmt::Display> fmt::Display for ResourceUpdateError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(f),
            Self::Update { type_name, source } => {
                write!(f, "could not update resource `{type_name}`: {source}")
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ResourceUpdateError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resource(error) => Some(error),
            Self::Update { source, .. } => Some(source),
        }
    }
}

#[derive(Default)]
pub struct Resources {
    values: HashMap<TypeId, Box<dyn Any>>,
}
impl Resources {
    /// Maximum number of distinct resource types; replacing a type uses no slot.
    pub const CAPACITY: usize = 256;

    /// Replace an existing value of T, returning it. Capacity failure returns the input.
    pub fn insert<T: Any>(&mut self, value: T) -> Result<Option<T>, T> {
        let id = TypeId::of::<T>();
        if !self.values.contains_key(&id) && self.values.len() >= Self::CAPACITY {
            return Err(value);
        }
        Ok(self
            .values
            .insert(id, Box::new(value))
            .map(|old| *old.downcast::<T>().expect("resource type invariant")))
    }

    /// Return the existing resource, or initialize and insert it exactly once.
    ///
    /// `initialize` is not called if `T` is present, even when the store is full.
    /// For an absent type at capacity, return [`ResourceError::Capacity`] without
    /// calling it. A panic in the initializer inserts nothing. Removing or
    /// clearing the resource permits it to be initialized again.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.get_or_insert_with(Vec::<u32>::new)?.push(7);
    /// let values = resources.get_or_insert_with(|| vec![99_u32])?;
    /// assert_eq!(values, &[7]);
    /// # Ok::<(), ResourceError>(())
    /// ```
    pub fn get_or_insert_with<T: Any>(
        &mut self,
        initialize: impl FnOnce() -> T,
    ) -> Result<&mut T, ResourceError> {
        let value = match self.entry::<T>()? {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Box::new(initialize())),
        };
        Ok(value.downcast_mut().expect("resource type invariant"))
    }

    /// Return the existing resource, or try to initialize and insert it.
    ///
    /// The initializer runs only when `T` is absent and capacity is available.
    /// Failure preserves the original error and inserts nothing, so the caller
    /// can retry. Initializer panics also insert nothing; side effects performed
    /// by the initializer itself are the caller's responsibility.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceInitError, Resources};
    ///
    /// struct TickRate(u32);
    /// let mut resources = Resources::default();
    /// let rate = resources.try_init(|| "60".parse::<u32>().map(TickRate))?;
    /// assert_eq!(rate.0, 60);
    /// // A later caller reuses it without parsing a second configuration.
    /// let rate = resources.try_init(|| "invalid".parse::<u32>().map(TickRate))?;
    /// assert_eq!(rate.0, 60);
    /// # Ok::<(), ResourceInitError<std::num::ParseIntError>>(())
    /// ```
    pub fn try_init<T: Any, E>(
        &mut self,
        initialize: impl FnOnce() -> Result<T, E>,
    ) -> Result<&mut T, ResourceInitError<E>> {
        let value = match self.entry::<T>().map_err(ResourceInitError::Resource)? {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let value = initialize().map_err(|source| ResourceInitError::Initialization {
                    type_name: type_name::<T>(),
                    source,
                })?;
                entry.insert(Box::new(value))
            }
        };
        Ok(value.downcast_mut().expect("resource type invariant"))
    }

    /// Build a checked replacement from a shared borrow, returning the old value.
    ///
    /// `update` runs exactly once if `T` exists, including at capacity. A missing
    /// type returns [`ResourceError::Missing`] inside [`ResourceUpdateError::Resource`]
    /// without calling it; use [`Self::try_init`] to initialize absent resources.
    /// An error or panic in `update` leaves the original value in its slot.
    /// Only `Ok(replacement)` commits the replacement, transferring ownership of
    /// the previous value to the caller without dropping it inside this method.
    ///
    /// The store reuses the existing allocation and never changes its type count.
    /// Neither `Clone` nor `Send`/`Sync` is required. The callback's own allocations,
    /// external side effects, and changes made through interior mutability are
    /// outside this guarantee and cannot be rolled back.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceUpdateError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.get_or_insert_with(|| 3_u32).unwrap();
    /// let result = resources.try_update(|lives: &u32| {
    ///     lives.checked_sub(4).ok_or("not enough lives")
    /// });
    /// assert!(matches!(result, Err(ResourceUpdateError::Update {
    ///     source: "not enough lives", ..
    /// })));
    /// assert_eq!(resources.get::<u32>(), Some(&3));
    /// let previous = resources.try_update(|lives: &u32| {
    ///     lives.checked_sub(1).ok_or("not enough lives")
    /// }).unwrap();
    /// assert_eq!(previous, 3);
    /// assert_eq!(resources.get::<u32>(), Some(&2));
    /// ```
    pub fn try_update<T: Any, E>(
        &mut self,
        update: impl FnOnce(&T) -> Result<T, E>,
    ) -> Result<T, ResourceUpdateError<E>> {
        let current = self
            .require_mut::<T>()
            .map_err(ResourceUpdateError::Resource)?;
        let replacement = update(current).map_err(|source| ResourceUpdateError::Update {
            type_name: type_name::<T>(),
            source,
        })?;
        Ok(std::mem::replace(current, replacement))
    }

    /// Borrow a required resource, reporting its Rust type name if absent.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceError, Resources};
    ///
    /// struct Settings { sound: bool }
    /// let mut resources = Resources::default();
    /// resources.get_or_insert_with(|| Settings { sound: true })?;
    /// assert!(resources.require::<Settings>()?.sound);
    /// assert!(matches!(resources.require::<u64>(), Err(ResourceError::Missing { .. })));
    /// # Ok::<(), ResourceError>(())
    /// ```
    pub fn require<T: Any>(&self) -> Result<&T, ResourceError> {
        self.get::<T>().ok_or(ResourceError::Missing {
            type_name: type_name::<T>(),
        })
    }

    /// Mutably borrow a required resource, reporting its Rust type name if absent.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.get_or_insert_with(|| 0_u64)?;
    /// *resources.require_mut::<u64>()? += 1;
    /// assert_eq!(*resources.require::<u64>()?, 1);
    /// # Ok::<(), ResourceError>(())
    /// ```
    pub fn require_mut<T: Any>(&mut self) -> Result<&mut T, ResourceError> {
        self.get_mut::<T>().ok_or(ResourceError::Missing {
            type_name: type_name::<T>(),
        })
    }

    /// Call `access` once with mutable references to two distinct resources.
    ///
    /// All types must be distinct and present before the callback runs; see
    /// [`ResourceAccessError`] for error precedence and argument indices. Access
    /// works at full capacity, never inserts/removes resources, and preserves
    /// their allocations. References cannot escape the callback's scope.
    ///
    /// The outer `Result` describes access validation only. If the callback
    /// returns a `Result`, it is returned inside `Ok`. Mutations made by the
    /// callback persist even if it returns an error or panics; this is not a
    /// transaction like [`Self::try_update`]. After an unwind, all entries remain
    /// installed and can be borrowed again. Shared interior mutations and other
    /// callback side effects are likewise not rolled back.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceAccessError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.insert(2_u32).unwrap();
    /// resources.insert(10_u64).unwrap();
    /// let total = resources.with2_mut(|lives: &mut u32, score: &mut u64| {
    ///     *lives -= 1;
    ///     *score += 5;
    ///     *score
    /// })?;
    /// assert_eq!(total, 15);
    /// assert_eq!(resources.get::<u32>(), Some(&1));
    /// # Ok::<(), ResourceAccessError>(())
    /// ```
    ///
    /// Resource borrows cannot be returned from the callback:
    ///
    /// ```compile_fail
    /// use ferravex_engine::resources::Resources;
    /// let mut resources = Resources::default();
    /// let escaped = resources.with2_mut::<u32, u64, _>(|a, _| a);
    /// ```
    pub fn with2_mut<A: Any, B: Any, R>(
        &mut self,
        access: impl FnOnce(&mut A, &mut B) -> R,
    ) -> Result<R, ResourceAccessError> {
        let [a, b] = self.disjoint([
            (TypeId::of::<A>(), type_name::<A>()),
            (TypeId::of::<B>(), type_name::<B>()),
        ])?;
        Ok(access(
            a.downcast_mut().expect("resource type invariant"),
            b.downcast_mut().expect("resource type invariant"),
        ))
    }

    /// Call `access` with mutable `A` and shared `B`, requiring distinct types.
    ///
    /// Uses [`Self::with2_mut`]'s validation, scope and side-effect contract.
    /// Reorder type arguments when the resource to mutate is otherwise second.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceAccessError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.insert(10_u64).unwrap();
    /// resources.insert(3_u32).unwrap();
    /// resources.with2_mut_ref(|score: &mut u64, reward: &u32| {
    ///     *score += u64::from(*reward);
    /// })?;
    /// assert_eq!(resources.get::<u64>(), Some(&13));
    /// assert_eq!(resources.get::<u32>(), Some(&3));
    /// # Ok::<(), ResourceAccessError>(())
    /// ```
    pub fn with2_mut_ref<A: Any, B: Any, R>(
        &mut self,
        access: impl FnOnce(&mut A, &B) -> R,
    ) -> Result<R, ResourceAccessError> {
        self.with2_mut::<A, B, R>(|a, b| access(a, b))
    }

    /// Call `access` with mutable references to three distinct resources.
    ///
    /// Uses [`Self::with2_mut`]'s validation, scope and side-effect contract.
    /// Missing argument indices are 0 (`A`), 1 (`B`) and 2 (`C`).
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceAccessError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.insert(2_u32).unwrap();
    /// resources.insert(10_u64).unwrap();
    /// resources.insert(false).unwrap();
    /// resources.with3_mut(|lives: &mut u32, score: &mut u64, won: &mut bool| {
    ///     *lives -= 1;
    ///     *score += 5;
    ///     *won = *score >= 15;
    /// })?;
    /// assert_eq!(resources.get::<bool>(), Some(&true));
    /// # Ok::<(), ResourceAccessError>(())
    /// ```
    pub fn with3_mut<A: Any, B: Any, C: Any, R>(
        &mut self,
        access: impl FnOnce(&mut A, &mut B, &mut C) -> R,
    ) -> Result<R, ResourceAccessError> {
        let [a, b, c] = self.disjoint([
            (TypeId::of::<A>(), type_name::<A>()),
            (TypeId::of::<B>(), type_name::<B>()),
            (TypeId::of::<C>(), type_name::<C>()),
        ])?;
        Ok(access(
            a.downcast_mut().expect("resource type invariant"),
            b.downcast_mut().expect("resource type invariant"),
            c.downcast_mut().expect("resource type invariant"),
        ))
    }

    /// Call `access` with mutable `A`/`B` and shared `C`, all distinct types.
    ///
    /// Uses [`Self::with2_mut`]'s validation, scope and side-effect contract.
    /// This recipe admits a job only if both the per-tick work allowance and the
    /// bounded accepted-job queue have room. The caller invokes it on an actual
    /// simulation tick and resets the budget at its chosen tick boundary.
    ///
    /// ```
    /// use ferravex_engine::{
    ///     events::{EventQueue, Overflow},
    ///     resources::{ResourceAccessError, Resources},
    ///     work_budget::WorkBudget,
    /// };
    ///
    /// struct Job { id: u32, cost: u64 }
    /// let mut resources = Resources::default();
    /// resources.get_or_insert_with(|| WorkBudget::new(3)).unwrap();
    /// resources.get_or_insert_with(|| EventQueue::<u32>::new(1, Overflow::RejectNewest))
    ///     .unwrap();
    /// resources.get_or_insert_with(|| Job { id: 7, cost: 2 }).unwrap();
    /// let admit = |budget: &mut WorkBudget, accepted: &mut EventQueue<u32>, job: &Job| {
    ///     if accepted.is_full() || !budget.try_consume(job.cost) {
    ///         return false;
    ///     }
    ///     accepted.send(job.id).expect("queue capacity checked");
    ///     true
    /// };
    /// assert!(resources.with3_mut_mut_ref(admit)?);
    /// assert!(!resources.with3_mut_mut_ref(admit)?); // Full: no charge, no event.
    /// assert_eq!(resources.get::<WorkBudget>().unwrap().remaining(), 1);
    /// assert_eq!(resources.get_mut::<EventQueue<u32>>().unwrap().pop(), Some(7));
    /// assert!(!resources.with3_mut_mut_ref(admit)?); // Budget exhausted.
    /// assert!(resources.get::<EventQueue<u32>>().unwrap().is_empty());
    /// # Ok::<(), ResourceAccessError>(())
    /// ```
    pub fn with3_mut_mut_ref<A: Any, B: Any, C: Any, R>(
        &mut self,
        access: impl FnOnce(&mut A, &mut B, &C) -> R,
    ) -> Result<R, ResourceAccessError> {
        self.with3_mut::<A, B, C, R>(|a, b, c| access(a, b, c))
    }

    /// Call `access` with mutable `A` and shared `B`/`C`, all distinct types.
    ///
    /// Uses [`Self::with2_mut`]'s validation, scope and side-effect contract.
    /// Even the two shared arguments must differ in type. Use separate shared
    /// [`Self::require`] calls when repeated shared access is intended.
    ///
    /// ```
    /// use ferravex_engine::resources::{ResourceAccessError, Resources};
    ///
    /// let mut resources = Resources::default();
    /// resources.insert(0_u64).unwrap();
    /// resources.insert(3_u32).unwrap();
    /// resources.insert(true).unwrap();
    /// resources.with3_mut_ref_ref(|score: &mut u64, reward: &u32, enabled: &bool| {
    ///     if *enabled { *score += u64::from(*reward); }
    /// })?;
    /// assert_eq!(resources.get::<u64>(), Some(&3));
    /// # Ok::<(), ResourceAccessError>(())
    /// ```
    pub fn with3_mut_ref_ref<A: Any, B: Any, C: Any, R>(
        &mut self,
        access: impl FnOnce(&mut A, &B, &C) -> R,
    ) -> Result<R, ResourceAccessError> {
        self.with3_mut::<A, B, C, R>(|a, b, c| access(a, b, c))
    }

    pub fn get<T: Any>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref()
    }
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.values.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }
    pub fn remove<T: Any>(&mut self) -> Option<T> {
        self.values
            .remove(&TypeId::of::<T>())
            .and_then(|old| old.downcast::<T>().ok())
            .map(|v| *v)
    }
    pub fn contains<T: Any>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<T>())
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn clear(&mut self) {
        self.values.clear();
    }

    fn disjoint<const N: usize>(
        &mut self,
        types: [(TypeId, &'static str); N],
    ) -> Result<[&mut dyn Any; N], ResourceAccessError> {
        // Validate TypeIds, not diagnostic names, before get_disjoint_mut:
        // duplicate keys would otherwise panic instead of returning an error.
        for second in 1..N {
            for first in 0..second {
                if types[first].0 == types[second].0 {
                    return Err(ResourceAccessError::Duplicate {
                        type_name: types[first].1,
                        first,
                        second,
                    });
                }
            }
        }
        let ids = types.map(|(id, _)| id);
        let values = self.values.get_disjoint_mut(ids.each_ref());
        for (index, value) in values.iter().enumerate() {
            if value.is_none() {
                return Err(ResourceAccessError::Missing {
                    type_name: types[index].1,
                    index,
                });
            }
        }
        Ok(values.map(|value| value.expect("validated resource presence").as_mut()))
    }

    fn entry<T: Any>(&mut self) -> Result<Entry<'_, TypeId, Box<dyn Any>>, ResourceError> {
        let full = self.values.len() >= Self::CAPACITY;
        match self.values.entry(TypeId::of::<T>()) {
            Entry::Vacant(_) if full => Err(ResourceError::Capacity {
                type_name: type_name::<T>(),
                capacity: Self::CAPACITY,
            }),
            entry => Ok(entry),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, error::Error, rc::Rc};

    #[derive(Debug, PartialEq, Eq)]
    struct Slot<const N: usize>(usize);

    fn full_resources() -> Resources {
        let mut resources = Resources::default();
        // Exercise the public API with 256 real, distinct types.
        macro_rules! fill {
            ($base:expr, []) => {
                assert_eq!(resources.insert(Slot::<{ $base }>($base)), Ok(None));
            };
            ($base:expr, [$step:expr $(, $rest:expr)*]) => {
                fill!($base, [$($rest),*]);
                fill!($base + $step, [$($rest),*]);
            };
        }
        fill!(0, [128, 64, 32, 16, 8, 4, 2, 1]);
        assert_eq!(resources.len(), Resources::CAPACITY);
        resources
    }

    #[test]
    fn lazy_setup_reuses_mutations_and_runs_again_only_after_removal_or_clear() {
        let mut resources = Resources::default();
        let calls = Cell::new(0);
        let initialize = || {
            calls.set(calls.get() + 1);
            vec![1_u32]
        };
        resources.get_or_insert_with(initialize).unwrap().push(2);
        assert_eq!(resources.get_or_insert_with(initialize).unwrap(), &[1, 2]);
        assert_eq!(calls.get(), 1);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources.remove::<Vec<u32>>(), Some(vec![1, 2]));
        assert_eq!(resources.get_or_insert_with(initialize).unwrap(), &[1]);
        assert_eq!(calls.get(), 2);
        resources.clear();
        resources.get_or_insert_with(initialize).unwrap();
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn lazy_setup_accepts_fn_once_and_non_send_resources_with_single_drop() {
        struct LocalResource(Rc<Cell<u32>>);
        impl Drop for LocalResource {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let resource = LocalResource(drops.clone());
        let mut resources = Resources::default();
        resources.get_or_insert_with(|| resource).unwrap();
        resources
            .get_or_insert_with(|| -> LocalResource { panic!("already initialized") })
            .unwrap();
        assert_eq!(drops.get(), 0);
        drop(resources.remove::<LocalResource>());
        assert_eq!(drops.get(), 1);
        resources.clear();
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn fallible_setup_preserves_failure_and_existing_resources_then_allows_retry() {
        let mut resources = Resources::default();
        resources.insert(19_u64).unwrap();
        let calls = Cell::new(0);
        let error = resources
            .try_init::<Vec<u32>, _>(|| {
                calls.set(calls.get() + 1);
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "settings",
                ))
            })
            .unwrap_err();
        assert!(matches!(
            &error,
            ResourceInitError::Initialization { type_name: name, source }
                if *name == type_name::<Vec<u32>>()
                    && source.kind() == std::io::ErrorKind::NotFound
        ));
        assert!(error.to_string().contains(type_name::<Vec<u32>>()));
        assert_eq!(error.source().unwrap().to_string(), "settings");
        assert_eq!(resources.len(), 1);
        assert!(!resources.contains::<Vec<u32>>());
        assert_eq!(resources.get::<u64>(), Some(&19));

        let initialize = || -> Result<Vec<u32>, std::io::Error> {
            calls.set(calls.get() + 1);
            Ok(vec![3])
        };
        resources.try_init(initialize).unwrap().push(4);
        assert_eq!(resources.try_init(initialize).unwrap(), &[3, 4]);
        assert_eq!(calls.get(), 2);
        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn fallible_setup_preserves_errors_without_error_trait_or_static_bounds() {
        struct BorrowedFailure<'a>(&'a str);
        let detail = String::from("retry after loading");
        let mut resources = Resources::default();
        match resources.try_init::<u32, _>(|| Err(BorrowedFailure(&detail))) {
            Err(ResourceInitError::Initialization {
                type_name: name,
                source,
            }) => {
                assert_eq!(name, type_name::<u32>());
                assert_eq!(source.0, detail);
            }
            _ => panic!("initializer error was not preserved"),
        }
        assert!(resources.is_empty());
    }

    #[test]
    fn full_store_skips_initializers_and_keeps_existing_access_and_insert_contract() {
        let mut resources = full_resources();
        let capacity_error = ResourceError::Capacity {
            type_name: type_name::<String>(),
            capacity: Resources::CAPACITY,
        };
        assert_eq!(
            resources.get_or_insert_with(|| -> String { panic!("no available slot") }),
            Err(capacity_error)
        );
        let error = resources
            .try_init::<String, std::io::Error>(|| panic!("no available slot"))
            .unwrap_err();
        assert!(matches!(error, ResourceInitError::Resource(e) if e == capacity_error));
        assert_eq!(
            error.source().unwrap().to_string(),
            capacity_error.to_string()
        );
        assert!(error.to_string().contains(type_name::<String>()));
        assert!(error.to_string().contains("256"));
        assert_eq!(resources.len(), Resources::CAPACITY);

        resources
            .get_or_insert_with(|| -> Slot<0> { panic!("already initialized") })
            .unwrap()
            .0 = 42;
        assert_eq!(
            resources
                .try_init::<Slot<0>, ()>(|| panic!("already initialized"))
                .unwrap(),
            &Slot::<0>(42)
        );
        assert_eq!(resources.insert(Slot::<0>(43)), Ok(Some(Slot::<0>(42))));
        assert_eq!(
            resources.insert(String::from("keep me")),
            Err(String::from("keep me"))
        );
        assert_eq!(resources.get::<Slot<255>>(), Some(&Slot::<255>(255)));

        assert_eq!(resources.remove::<Slot<0>>(), Some(Slot::<0>(43)));
        assert_eq!(
            resources
                .try_init(|| Ok::<_, ()>(String::from("loaded")))
                .unwrap(),
            "loaded"
        );
        assert_eq!(resources.len(), Resources::CAPACITY);
    }

    #[test]
    fn failed_or_panicking_initializers_leave_the_last_slot_available() {
        let mut resources = full_resources();
        resources.remove::<Slot<0>>().unwrap();
        assert!(resources.try_init::<String, _>(|| Err("failed")).is_err());
        assert_eq!(resources.len(), Resources::CAPACITY - 1);
        assert!(!resources.contains::<String>());

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = resources.get_or_insert_with(|| -> String { panic!("setup panic") });
        }));
        assert!(panic.is_err());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = resources.try_init::<String, ()>(|| panic!("fallible setup panic"));
        }));
        assert!(panic.is_err());
        assert_eq!(resources.len(), Resources::CAPACITY - 1);
        assert!(!resources.contains::<String>());
        assert_eq!(resources.get::<Slot<255>>(), Some(&Slot::<255>(255)));
        resources
            .get_or_insert_with(String::new)
            .unwrap()
            .push_str("retry");
        assert_eq!(resources.require::<String>().unwrap(), "retry");
        assert_eq!(resources.len(), Resources::CAPACITY);
    }

    #[test]
    fn required_access_reports_exact_missing_types_without_inserting() {
        struct Settings;
        struct OtherSettings;
        let mut resources = Resources::default();
        let expected = ResourceError::Missing {
            type_name: type_name::<Settings>(),
        };
        assert_eq!(resources.require::<Settings>().err(), Some(expected));
        assert_eq!(resources.require_mut::<Settings>().err(), Some(expected));
        assert_ne!(resources.require::<OtherSettings>().err(), Some(expected));
        assert!(resources.is_empty());
        assert!(expected.to_string().contains(type_name::<Settings>()));
        assert!(expected.to_string().contains("insert or initialize"));
        assert!(expected.source().is_none());

        resources.insert(5_u32).unwrap();
        assert_eq!(resources.require::<u32>(), Ok(&5));
        *resources.require_mut::<u32>().unwrap() = 9;
        assert_eq!(resources.require::<u32>(), Ok(&9));
        resources.remove::<u32>();
        assert_eq!(
            resources.require::<u32>(),
            Err(ResourceError::Missing {
                type_name: type_name::<u32>()
            })
        );
    }

    #[test]
    fn checked_update_preserves_failure_and_old_value_then_allows_retry() {
        let mut resources = Resources::default();
        resources.insert(3_u32).unwrap();
        resources.insert(19_u64).unwrap();
        let calls = Cell::new(0);
        let error = resources
            .try_update(|lives: &u32| {
                calls.set(calls.get() + 1);
                lives.checked_sub(4).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "not enough lives")
                })
            })
            .unwrap_err();
        assert!(matches!(
            &error,
            ResourceUpdateError::Update { type_name: name, source }
                if *name == type_name::<u32>()
                    && source.kind() == std::io::ErrorKind::InvalidInput
        ));
        assert_eq!(
            error.to_string(),
            format!(
                "could not update resource `{}`: not enough lives",
                type_name::<u32>()
            )
        );
        assert_eq!(error.source().unwrap().to_string(), "not enough lives");
        assert!(error.source().unwrap().is::<std::io::Error>());
        assert_eq!(resources.get::<u32>(), Some(&3));
        assert_eq!(resources.get::<u64>(), Some(&19));
        assert_eq!(resources.len(), 2);
        assert_eq!(calls.get(), 1);

        let previous = resources
            .try_update(|lives: &u32| {
                calls.set(calls.get() + 1);
                lives.checked_sub(1).ok_or("not enough lives")
            })
            .unwrap();
        assert_eq!(previous, 3);
        assert_eq!(resources.get::<u32>(), Some(&2));
        assert_eq!(resources.get::<u64>(), Some(&19));
        assert_eq!(resources.len(), 2);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn checked_update_reports_missing_types_without_running_the_updater() {
        let mut resources = Resources::default();
        let expected = ResourceError::Missing {
            type_name: type_name::<String>(),
        };
        let error = resources
            .try_update::<String, std::io::Error>(|_| panic!("missing resource"))
            .unwrap_err();
        assert!(matches!(error, ResourceUpdateError::Resource(e) if e == expected));
        assert_eq!(error.to_string(), expected.to_string());
        assert_eq!(
            error.source().unwrap().downcast_ref::<ResourceError>(),
            Some(&expected)
        );
        assert_eq!(
            resources.try_update::<u64, ()>(|_| panic!("missing resource")),
            Err(ResourceUpdateError::Resource(ResourceError::Missing {
                type_name: type_name::<u64>(),
            }))
        );
        assert!(resources.is_empty());

        resources.get_or_insert_with(String::new).unwrap();
        resources.remove::<String>().unwrap();
        assert_eq!(
            resources.try_update::<String, ()>(|_| panic!("removed resource")),
            Err(ResourceUpdateError::Resource(expected))
        );
        assert!(resources.is_empty());
    }

    #[test]
    fn checked_update_preserves_capacity_and_never_inserts_missing_types() {
        let mut resources = full_resources();
        assert_eq!(
            resources.try_update::<Slot<0>, _>(|current| {
                assert_eq!(current, &Slot::<0>(0));
                Err("invalid configuration")
            }),
            Err(ResourceUpdateError::Update {
                type_name: type_name::<Slot<0>>(),
                source: "invalid configuration",
            })
        );
        assert_eq!(resources.get::<Slot<0>>(), Some(&Slot::<0>(0)));
        assert_eq!(resources.len(), Resources::CAPACITY);
        assert_eq!(
            resources.try_update(|current: &Slot<0>| Ok::<_, ()>(Slot::<0>(current.0 + 7))),
            Ok(Slot::<0>(0))
        );
        assert_eq!(resources.get::<Slot<0>>(), Some(&Slot::<0>(7)));
        assert_eq!(resources.get::<Slot<255>>(), Some(&Slot::<255>(255)));
        assert_eq!(resources.len(), Resources::CAPACITY);
        assert_eq!(
            resources.try_update::<String, ()>(|_| panic!("missing even at capacity")),
            Err(ResourceUpdateError::Resource(ResourceError::Missing {
                type_name: type_name::<String>(),
            }))
        );
        assert_eq!(
            resources.insert(String::from("full")),
            Err(String::from("full"))
        );
        assert_eq!(resources.remove::<Slot<0>>(), Some(Slot::<0>(7)));
        assert_eq!(
            resources.try_init(|| Ok::<_, ()>(String::from("loaded"))),
            Ok(&mut String::from("loaded"))
        );
        assert_eq!(resources.len(), Resources::CAPACITY);
    }

    #[test]
    fn checked_update_preserves_borrowed_errors_without_trait_bounds() {
        struct BorrowedFailure<'a>(&'a str);
        let detail = String::from("retry after loading");
        let mut resources = Resources::default();
        resources.insert(8_u32).unwrap();
        match resources.try_update::<u32, _>(|_| Err(BorrowedFailure(&detail))) {
            Err(ResourceUpdateError::Update {
                type_name: name,
                source,
            }) => {
                assert_eq!(name, type_name::<u32>());
                assert_eq!(source.0, detail);
            }
            _ => panic!("update error was not preserved"),
        }
        assert_eq!(resources.get::<u32>(), Some(&8));
        assert_eq!(resources.len(), 1);
    }

    #[test]
    fn checked_update_reuses_storage_and_returns_non_clone_local_resources_without_dropping() {
        struct LocalResource {
            value: u32,
            drops: Rc<Cell<u32>>,
        }
        impl Drop for LocalResource {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }
        let old_drops = Rc::new(Cell::new(0));
        let new_drops = Rc::new(Cell::new(0));
        let mut resources = Resources::default();
        resources
            .get_or_insert_with(|| LocalResource {
                value: 7,
                drops: old_drops.clone(),
            })
            .unwrap();
        let address = std::ptr::from_ref(resources.get::<LocalResource>().unwrap());
        assert!(
            resources
                .try_update::<LocalResource, _>(|_| Err("invalid"))
                .is_err()
        );
        assert_eq!(old_drops.get(), 0);
        assert_eq!(
            std::ptr::from_ref(resources.get::<LocalResource>().unwrap()),
            address
        );

        let replacement = LocalResource {
            value: 9,
            drops: new_drops.clone(),
        };
        let previous = resources
            .try_update(|current: &LocalResource| {
                assert_eq!(current.value, 7);
                Ok::<_, ()>(replacement)
            })
            .unwrap();
        assert_eq!(previous.value, 7);
        assert_eq!(resources.get::<LocalResource>().unwrap().value, 9);
        assert_eq!(
            std::ptr::from_ref(resources.get::<LocalResource>().unwrap()),
            address
        );
        assert_eq!(old_drops.get(), 0);
        assert_eq!(new_drops.get(), 0);
        drop(previous);
        assert_eq!(old_drops.get(), 1);
        resources.clear();
        assert_eq!(old_drops.get(), 1);
        assert_eq!(new_drops.get(), 1);
        assert!(resources.is_empty());
    }

    #[test]
    fn panicking_update_keeps_the_original_allocation_available_for_retry() {
        let mut resources = Resources::default();
        resources.insert(vec![3_u32, 4]).unwrap();
        resources.insert(11_u64).unwrap();
        let address = resources.get::<Vec<u32>>().unwrap().as_ptr();
        let calls = Cell::new(0);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = resources.try_update::<Vec<u32>, ()>(|values| {
                calls.set(calls.get() + 1);
                assert_eq!(values, &[3, 4]);
                panic!("replacement setup panic");
            });
        }));
        assert!(panic.is_err());
        assert_eq!(calls.get(), 1);
        assert_eq!(resources.get::<Vec<u32>>().unwrap().as_ptr(), address);
        assert_eq!(resources.get::<Vec<u32>>().unwrap(), &[3, 4]);
        assert_eq!(resources.get::<u64>(), Some(&11));
        assert_eq!(resources.len(), 2);
        let previous = resources
            .try_update(|values: &Vec<u32>| Ok::<_, ()>(vec![values.iter().sum()]))
            .unwrap();
        assert_eq!(previous.as_ptr(), address);
        assert_eq!(previous, [3, 4]);
        assert_eq!(resources.get::<Vec<u32>>().unwrap(), &[7]);
    }

    #[test]
    fn checked_update_cannot_roll_back_callback_interior_mutability() {
        let mut resources = Resources::default();
        resources.insert(Cell::new(3_u32)).unwrap();
        assert_eq!(
            resources.try_update::<Cell<u32>, _>(|current| {
                current.set(4);
                Err("rejected replacement")
            }),
            Err(ResourceUpdateError::Update {
                type_name: type_name::<Cell<u32>>(),
                source: "rejected replacement",
            })
        );
        assert_eq!(resources.get::<Cell<u32>>().unwrap().get(), 4);
        assert_eq!(resources.len(), 1);
    }

    #[test]
    fn types_replace_independently_and_resources_drop_once() {
        struct Guard(Rc<Cell<u32>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let mut r = Resources::default();
        assert_eq!(r.insert(3_u32), Ok(None));
        assert_eq!(r.insert(4_u64), Ok(None));
        assert_eq!(r.insert(5_u32), Ok(Some(3)));
        *r.get_mut::<u64>().unwrap() = 9;
        assert_eq!(r.get::<u32>(), Some(&5));
        assert_eq!(r.remove::<u64>(), Some(9));
        let drops = Rc::new(Cell::new(0));
        assert!(r.insert(Guard(drops.clone())).is_ok());
        assert_eq!(drops.get(), 0);
        r.clear();
        assert_eq!(drops.get(), 1);
        assert!(r.is_empty());
        assert!(r.get::<u32>().is_none());
    }

    #[test]
    fn disjoint_access_rejects_aliases_and_every_duplicate_pair_before_presence() {
        type Alias = u32;
        for populated in [false, true] {
            let mut resources = Resources::default();
            if populated {
                resources.insert(7_u32).unwrap();
                resources.insert(11_u64).unwrap();
            }
            let pair_error = Err(ResourceAccessError::Duplicate {
                type_name: type_name::<u32>(),
                first: 0,
                second: 1,
            });
            assert_eq!(
                resources.with2_mut::<u32, Alias, ()>(|_, _| panic!("duplicate")),
                pair_error
            );
            assert_eq!(
                resources.with2_mut_ref::<u32, Alias, ()>(|_, _| panic!("duplicate")),
                pair_error
            );
            macro_rules! reject_three {
                ($a:ty, $b:ty, $c:ty, $first:expr, $second:expr) => {{
                    let expected = Err(ResourceAccessError::Duplicate {
                        type_name: type_name::<u32>(),
                        first: $first,
                        second: $second,
                    });
                    assert_eq!(
                        resources.with3_mut::<$a, $b, $c, ()>(|_, _, _| panic!("duplicate")),
                        expected
                    );
                    assert_eq!(
                        resources
                            .with3_mut_mut_ref::<$a, $b, $c, ()>(|_, _, _| panic!("duplicate")),
                        expected
                    );
                    assert_eq!(
                        resources
                            .with3_mut_ref_ref::<$a, $b, $c, ()>(|_, _, _| panic!("duplicate")),
                        expected
                    );
                }};
            }
            reject_three!(u32, Alias, u64, 0, 1);
            reject_three!(u32, u64, Alias, 0, 2);
            reject_three!(u64, u32, Alias, 1, 2);
            reject_three!(u32, Alias, u32, 0, 1);
            // A missing first argument does not mask a duplicate shared pair.
            reject_three!(String, u32, Alias, 1, 2);
            assert_eq!(resources.len(), if populated { 2 } else { 0 });
            assert_eq!(resources.get::<u32>().copied(), populated.then_some(7));
            assert_eq!(resources.get::<u64>().copied(), populated.then_some(11));
        }
    }

    #[test]
    fn disjoint_pair_checks_every_missing_subset_without_callback_or_mutation() {
        for present in 0_u8..3 {
            let mut resources = Resources::default();
            resources.insert("unrelated").unwrap();
            if present & 1 != 0 {
                resources.insert(7_u32).unwrap();
            }
            if present & 2 != 0 {
                resources.insert(11_u64).unwrap();
            }
            let index = if present & 1 == 0 { 0 } else { 1 };
            let expected = Err(ResourceAccessError::Missing {
                type_name: [type_name::<u32>(), type_name::<u64>()][index],
                index,
            });
            assert_eq!(
                resources.with2_mut::<u32, u64, ()>(|_, _| panic!("missing")),
                expected
            );
            assert_eq!(
                resources.with2_mut_ref::<u32, u64, ()>(|_, _| panic!("missing")),
                expected
            );
            assert_eq!(resources.len(), present.count_ones() as usize + 1);
            assert_eq!(
                resources.get::<u32>().copied(),
                (present & 1 != 0).then_some(7)
            );
            assert_eq!(
                resources.get::<u64>().copied(),
                (present & 2 != 0).then_some(11)
            );
            assert_eq!(resources.get::<&str>(), Some(&"unrelated"));
        }
    }

    #[test]
    fn disjoint_triple_checks_every_missing_subset_without_callback_or_mutation() {
        for present in 0_u8..7 {
            let mut resources = Resources::default();
            resources.insert("unrelated").unwrap();
            if present & 1 != 0 {
                resources.insert(7_u32).unwrap();
            }
            if present & 2 != 0 {
                resources.insert(11_u64).unwrap();
            }
            if present & 4 != 0 {
                resources.insert(true).unwrap();
            }
            let index = (0..3).find(|i| present & (1 << i) == 0).unwrap();
            let expected = Err(ResourceAccessError::Missing {
                type_name: [type_name::<u32>(), type_name::<u64>(), type_name::<bool>()][index],
                index,
            });
            assert_eq!(
                resources.with3_mut::<u32, u64, bool, ()>(|_, _, _| panic!("missing")),
                expected
            );
            assert_eq!(
                resources.with3_mut_mut_ref::<u32, u64, bool, ()>(|_, _, _| panic!("missing")),
                expected
            );
            assert_eq!(
                resources.with3_mut_ref_ref::<u32, u64, bool, ()>(|_, _, _| panic!("missing")),
                expected
            );
            assert_eq!(resources.len(), present.count_ones() as usize + 1);
            assert_eq!(
                resources.get::<u32>().copied(),
                (present & 1 != 0).then_some(7)
            );
            assert_eq!(
                resources.get::<u64>().copied(),
                (present & 2 != 0).then_some(11)
            );
            assert_eq!(
                resources.get::<bool>().copied(),
                (present & 4 != 0).then_some(true)
            );
            assert_eq!(resources.get::<&str>(), Some(&"unrelated"));
        }
    }

    #[test]
    fn disjoint_access_preserves_requested_order_for_all_modes_and_permutations() {
        fn exercise<const A: usize, const B: usize, const C: usize>() {
            let mut resources = Resources::default();
            resources.insert(Slot::<0>(1)).unwrap();
            resources.insert(Slot::<1>(10)).unwrap();
            resources.insert(Slot::<2>(100)).unwrap();
            let expected = [1, 10, 100];
            let calls = Cell::new(0);
            let out = resources.with3_mut(|a: &mut Slot<A>, b: &mut Slot<B>, c: &mut Slot<C>| {
                calls.set(calls.get() + 1);
                assert_eq!((a.0, b.0, c.0), (expected[A], expected[B], expected[C]));
                a.0 = 7;
                b.0 = 8;
                c.0 = 9;
                "executed"
            });
            assert_eq!(out, Ok("executed"));
            resources
                .with2_mut(|a: &mut Slot<A>, b: &mut Slot<B>| {
                    calls.set(calls.get() + 1);
                    assert_eq!((a.0, b.0), (7, 8));
                    a.0 += 1;
                    b.0 += 2;
                })
                .unwrap();
            resources
                .with2_mut_ref(|a: &mut Slot<A>, b: &Slot<B>| {
                    calls.set(calls.get() + 1);
                    a.0 += b.0;
                })
                .unwrap();
            resources
                .with3_mut_mut_ref(|a: &mut Slot<A>, b: &mut Slot<B>, c: &Slot<C>| {
                    calls.set(calls.get() + 1);
                    a.0 += c.0;
                    b.0 += c.0;
                })
                .unwrap();
            resources
                .with3_mut_ref_ref(|a: &mut Slot<A>, b: &Slot<B>, c: &Slot<C>| {
                    calls.set(calls.get() + 1);
                    a.0 += b.0 + c.0;
                })
                .unwrap();
            assert_eq!(calls.get(), 5);
            assert_eq!(resources.get::<Slot<A>>(), Some(&Slot::<A>(55)));
            assert_eq!(resources.get::<Slot<B>>(), Some(&Slot::<B>(19)));
            assert_eq!(resources.get::<Slot<C>>(), Some(&Slot::<C>(9)));
            assert_eq!(resources.len(), 3);
        }
        exercise::<0, 1, 2>();
        exercise::<0, 2, 1>();
        exercise::<1, 0, 2>();
        exercise::<1, 2, 0>();
        exercise::<2, 0, 1>();
        exercise::<2, 1, 0>();
    }

    #[test]
    fn disjoint_access_at_capacity_reuses_storage_and_preserves_admission() {
        let mut resources = full_resources();
        let map_capacity = resources.values.capacity();
        let first = std::ptr::from_ref(resources.get::<Slot<0>>().unwrap());
        let middle = std::ptr::from_ref(resources.get::<Slot<128>>().unwrap());
        let last = std::ptr::from_ref(resources.get::<Slot<255>>().unwrap());
        resources
            .with3_mut(|a: &mut Slot<0>, b: &mut Slot<128>, c: &mut Slot<255>| {
                assert_eq!(std::ptr::from_ref(a), first);
                assert_eq!(std::ptr::from_ref(b), middle);
                assert_eq!(std::ptr::from_ref(c), last);
                a.0 += 1;
                b.0 += 1;
                c.0 += 1;
            })
            .unwrap();
        resources
            .with2_mut_ref(|a: &mut Slot<0>, b: &Slot<128>| a.0 += b.0)
            .unwrap();
        assert_eq!(resources.get::<Slot<0>>(), Some(&Slot::<0>(130)));
        assert_eq!(resources.get::<Slot<1>>(), Some(&Slot::<1>(1)));
        assert_eq!(resources.get::<Slot<254>>(), Some(&Slot::<254>(254)));
        assert_eq!(resources.get::<Slot<255>>(), Some(&Slot::<255>(256)));
        assert_eq!(
            resources.with3_mut_ref_ref::<Slot<0>, Slot<255>, String, ()>(|_, _, _| panic!(
                "missing at capacity"
            )),
            Err(ResourceAccessError::Missing {
                type_name: type_name::<String>(),
                index: 2
            })
        );
        assert_eq!(
            resources.with2_mut::<Slot<0>, Slot<0>, ()>(|_, _| panic!("duplicate at capacity")),
            Err(ResourceAccessError::Duplicate {
                type_name: type_name::<Slot<0>>(),
                first: 0,
                second: 1
            })
        );
        assert_eq!(resources.len(), Resources::CAPACITY);
        assert_eq!(resources.values.capacity(), map_capacity);
        assert_eq!(
            std::ptr::from_ref(resources.get::<Slot<0>>().unwrap()),
            first
        );
        assert_eq!(
            std::ptr::from_ref(resources.get::<Slot<128>>().unwrap()),
            middle
        );
        assert_eq!(
            std::ptr::from_ref(resources.get::<Slot<255>>().unwrap()),
            last
        );
        assert_eq!(
            resources.insert(String::from("full")),
            Err(String::from("full"))
        );
        resources.remove::<Slot<0>>().unwrap();
        resources.get_or_insert_with(String::new).unwrap();
        resources
            .with2_mut(|text: &mut String, n: &mut Slot<128>| {
                text.push_str("reused");
                n.0 += 1;
            })
            .unwrap();
        assert_eq!(resources.len(), Resources::CAPACITY);
        assert_eq!(resources.get::<String>().unwrap(), "reused");
        assert_eq!(resources.get::<Slot<128>>(), Some(&Slot::<128>(130)));
    }

    #[test]
    fn disjoint_access_supports_fn_once_borrowed_results_and_nontransactional_errors() {
        struct BorrowedFailure<'a>(&'a str);
        let detail = String::from("caller failure");
        let payload = String::from("owned capture");
        let mut resources = Resources::default();
        resources.insert(1_u32).unwrap();
        resources.insert(2_u64).unwrap();
        let result = resources.with2_mut(|a: &mut u32, b: &mut u64| {
            *a += 1;
            *b += 1;
            drop(payload); // Consuming a capture only requires FnOnce.
            Err::<(), _>(BorrowedFailure(&detail))
        });
        match result {
            Ok(Err(BorrowedFailure(message))) => assert_eq!(message, detail),
            _ => panic!("callback result was not preserved"),
        }
        assert_eq!(resources.get::<u32>(), Some(&2));
        assert_eq!(resources.get::<u64>(), Some(&3));
        assert_eq!(
            resources.with2_mut_ref::<u32, u64, _>(|_, _| detail.as_str()),
            Ok("caller failure")
        );
    }

    #[test]
    fn disjoint_callback_unwind_retains_entries_and_mutations_then_allows_retry() {
        let mut resources = Resources::default();
        resources.insert(1_u32).unwrap();
        resources.insert(2_u64).unwrap();
        resources.insert(Cell::new(3_u8)).unwrap();
        let first = std::ptr::from_ref(resources.get::<u32>().unwrap());
        let second = std::ptr::from_ref(resources.get::<u64>().unwrap());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = resources.with3_mut_mut_ref(|a: &mut u32, b: &mut u64, c: &Cell<u8>| {
                *a += 4;
                *b += 4;
                c.set(7);
                panic!("callback panic");
            });
        }));
        assert!(panic.is_err());
        assert_eq!(resources.len(), 3);
        assert_eq!(std::ptr::from_ref(resources.get::<u32>().unwrap()), first);
        assert_eq!(std::ptr::from_ref(resources.get::<u64>().unwrap()), second);
        assert_eq!(resources.get::<u32>(), Some(&5));
        assert_eq!(resources.get::<u64>(), Some(&6));
        assert_eq!(resources.get::<Cell<u8>>().unwrap().get(), 7);
        assert_eq!(
            resources.with3_mut_ref_ref(|a: &mut u32, b: &u64, c: &Cell<u8>| {
                *a += u32::from(c.get());
                *b
            }),
            Ok(6)
        );
        assert_eq!(resources.get::<u32>(), Some(&12));
    }

    #[test]
    fn disjoint_access_drops_non_clone_local_resources_once_and_resets_after_clear() {
        struct Local<const N: usize>(Rc<Cell<usize>>);
        impl<const N: usize> Drop for Local<N> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut resources = Resources::default();
        resources
            .get_or_insert_with(|| Local::<0>(drops.clone()))
            .unwrap();
        resources
            .get_or_insert_with(|| Local::<1>(drops.clone()))
            .unwrap();
        resources
            .get_or_insert_with(|| Local::<2>(drops.clone()))
            .unwrap();
        resources
            .with3_mut(|_: &mut Local<0>, _: &mut Local<1>, _: &mut Local<2>| {})
            .unwrap();
        assert_eq!(drops.get(), 0);
        drop(resources.remove::<Local<1>>());
        assert_eq!(drops.get(), 1);
        assert_eq!(
            resources.with3_mut::<Local<0>, Local<1>, Local<2>, ()>(|_, _, _| panic!("removed")),
            Err(ResourceAccessError::Missing {
                type_name: type_name::<Local<1>>(),
                index: 1
            })
        );
        resources.clear();
        assert_eq!(drops.get(), 3);
        assert!(resources.is_empty());
        assert_eq!(
            resources.with2_mut::<Local<0>, Local<2>, ()>(|_, _| panic!("cleared")),
            Err(ResourceAccessError::Missing {
                type_name: type_name::<Local<0>>(),
                index: 0
            })
        );
        resources
            .get_or_insert_with(|| Local::<0>(drops.clone()))
            .unwrap();
        resources
            .get_or_insert_with(|| Local::<2>(drops.clone()))
            .unwrap();
        resources
            .with2_mut_ref(|a: &mut Local<0>, b: &Local<2>| {
                assert!(Rc::ptr_eq(&a.0, &b.0));
            })
            .unwrap();
        drop(resources);
        assert_eq!(drops.get(), 5);
    }

    #[test]
    fn disjoint_access_errors_are_actionable_and_leave_resource_error_compatible() {
        let missing = ResourceAccessError::Missing {
            type_name: type_name::<u32>(),
            index: 1,
        };
        let duplicate = ResourceAccessError::Duplicate {
            type_name: type_name::<u64>(),
            first: 0,
            second: 2,
        };
        assert_eq!(
            missing.to_string(),
            "required resource `u32` at argument 1 is missing; insert or initialize it before use"
        );
        assert_eq!(
            duplicate.to_string(),
            "resource `u64` is requested at both argument 0 and 2; disjoint access requires distinct types"
        );
        assert!(missing.source().is_none());
        assert!(duplicate.source().is_none());
        // Existing exhaustive matches remain valid; no variant was added.
        let old_error = ResourceError::Missing { type_name: "u32" };
        assert_eq!(
            match old_error {
                ResourceError::Missing { type_name } => type_name,
                ResourceError::Capacity { type_name, .. } => type_name,
            },
            "u32"
        );
    }
}
