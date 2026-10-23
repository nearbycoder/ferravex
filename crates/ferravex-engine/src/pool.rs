//! Bounded reusable object slots with generation-checked handles.
//!
//! Handles are scoped to the originating pool: callers must not use them with
//! another pool, since pool identity is not stored or checked. Removing a value
//! invalidates its handle; exhausted generations permanently retire a slot.
//!
//! [`Pool::capacity`](crate::pool::Pool::capacity) is the configured slot limit, while [`Pool::available`](crate::pool::Pool::available)
//! counts slots that can still accept values, excluding retired slots. Storage
//! grows lazily: insertion can grow the slot vector, and removal (including
//! [`Pool::retain`](crate::pool::Pool::retain) and [`Pool::clear`](crate::pool::Pool::clear)) can grow the reusable-index vector.
//! Neither vector is shrunk on removal, so subsequent reuse within their warmed
//! capacities does not allocate pool storage. Iteration allocates no storage.
//! Selected mutable access through [`Pool::get2_mut`](crate::pool::Pool::get2_mut) or [`Pool::get_many_mut`](crate::pool::Pool::get_many_mut)
//! also allocates no storage, even before the reusable-index vector is warmed.

/// A generation-checked reference to a value in its originating [`Pool`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    index: usize,
    generation: u64,
}

/// A failed selection for [`Pool::get2_mut`] or [`Pool::get_many_mut`].
///
/// Positions are zero-based in the caller's selection, not pool slot indices.
/// All handles are checked for liveness in caller order first. Only if every
/// handle is live are duplicates checked, reporting the earliest repeated
/// position and its first occurrence. Failure leaves the pool unchanged.
/// Pool identity is not checked; handles must belong to the originating pool.
///
/// ```
/// use ferravex_engine::pool::{Pool, PoolAccessError};
///
/// let mut pool = Pool::new(1);
/// let actor = pool.insert(10).unwrap();
/// assert_eq!(pool.get2_mut(actor, actor),
///     Err(PoolAccessError::DuplicateHandle { first: 0, second: 1 }));
/// pool.clear();
/// assert_eq!(pool.get_many_mut([actor]),
///     Err(PoolAccessError::InvalidHandle { position: 0 }));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolAccessError {
    /// The handle is out of range, vacant, retired, or has a stale generation.
    InvalidHandle {
        /// Position of the first invalid handle in the selection.
        position: usize,
    },
    /// Two live handles select the same slot.
    DuplicateHandle {
        /// Position of the first occurrence.
        first: usize,
        /// Position of the earliest repeated handle.
        second: usize,
    },
}

impl std::fmt::Display for PoolAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHandle { position } => {
                write!(f, "invalid pool handle at selection position {position}")
            }
            Self::DuplicateHandle { first, second } => {
                write!(
                    f,
                    "duplicate pool handle at selection positions {first} and {second}"
                )
            }
        }
    }
}

impl std::error::Error for PoolAccessError {}

struct Slot<T> {
    generation: u64,
    value: Option<T>,
}
/// A lazily allocated pool with a fixed slot limit and stable live handles.
///
/// Values are never moved between slots by removal or retention. The backing
/// vector can move when it grows, so handles provide identity, not pinned addresses.
pub struct Pool<T> {
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    capacity: usize,
    len: usize,
}
impl<T> Pool<T> {
    /// Create an empty pool without allocating storage. A zero limit is valid.
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            capacity,
            len: 0,
        }
    }
    /// The configured slot limit, including slots retired by generation exhaustion.
    ///
    /// This limit never changes and is not the currently allocated storage size.
    /// Use [`Self::available`] to plan how many insertions can succeed.
    ///
    /// ```
    /// use ferravex_engine::pool::Pool;
    ///
    /// let mut pool = Pool::new(2);
    /// let handle = pool.insert("temporary").unwrap();
    /// pool.remove(handle);
    /// assert_eq!(pool.capacity(), 2);
    /// ```
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Number of insertions the pool can accept without intervening removals.
    ///
    /// Counts reusable and not-yet-created slots in constant time. Retired slots
    /// are excluded, so this can be less than `capacity() - len()`. Availability
    /// describes the slot limit; it does not promise allocation-free insertion.
    ///
    /// ```
    /// use ferravex_engine::pool::Pool;
    ///
    /// let mut pool = Pool::new(2);
    /// let handle = pool.insert(10).unwrap();
    /// assert_eq!(pool.available(), 1);
    /// pool.remove(handle);
    /// assert_eq!(pool.available(), 2);
    /// ```
    pub fn available(&self) -> usize {
        (self.capacity - self.slots.len()) + self.free.len()
    }
    /// Whether no further insertion can succeed without removing a live value.
    ///
    /// A zero-capacity pool, or one whose slots are all retired, is both empty
    /// and full. Removing a value at its final generation does not free a slot.
    ///
    /// ```
    /// use ferravex_engine::pool::Pool;
    ///
    /// let mut pool = Pool::new(1);
    /// pool.insert(10).unwrap();
    /// assert!(pool.is_full());
    /// assert_eq!(pool.insert(20), Err(20));
    /// assert!(Pool::<u8>::new(0).is_full());
    /// ```
    pub fn is_full(&self) -> bool {
        self.available() == 0
    }
    /// Insert a value, reusing the most recently freed slot when possible.
    ///
    /// Returns the value unchanged if no usable slot remains.
    pub fn insert(&mut self, value: T) -> Result<Handle, T> {
        let index = if let Some(i) = self.free.pop() {
            i
        } else {
            if self.slots.len() >= self.capacity {
                return Err(value);
            }
            self.slots.push(Slot {
                generation: 0,
                value: None,
            });
            self.slots.len() - 1
        };
        let slot = &mut self.slots[index];
        slot.value = Some(value);
        self.len += 1;
        Ok(Handle {
            index,
            generation: slot.generation,
        })
    }
    pub fn get(&self, h: Handle) -> Option<&T> {
        let s = self.slots.get(h.index)?;
        (s.generation == h.generation)
            .then_some(s.value.as_ref())
            .flatten()
    }
    pub fn get_mut(&mut self, h: Handle) -> Option<&mut T> {
        let s = self.slots.get_mut(h.index)?;
        if s.generation != h.generation {
            return None;
        }
        s.value.as_mut()
    }
    /// Borrow two live values mutably in the requested order without allocation.
    ///
    /// Both handles must belong to this pool and select distinct live slots.
    /// Validation and error precedence match [`Self::get_many_mut`]. No values
    /// are removed or moved, and their handles remain valid. This also works
    /// when the pool is full.
    ///
    /// This fixed-tick recipe transfers energy and resolves a one-dimensional
    /// equal-mass collision between two live actors. Feedback can follow the
    /// successful response; a rejected selection applies neither change.
    ///
    /// ```
    /// use ferravex_engine::pool::{Pool, PoolAccessError};
    ///
    /// #[derive(Debug)]
    /// struct Actor { energy: u32, velocity: i32 }
    /// let mut actors = Pool::new(2);
    /// let left = actors.insert(Actor { energy: 12, velocity: 3 }).unwrap();
    /// let right = actors.insert(Actor { energy: 2, velocity: -1 }).unwrap();
    /// let (donor, receiver) = actors.get2_mut(left, right)?;
    /// let transfer = donor.energy.min(4);
    /// donor.energy -= transfer;
    /// receiver.energy += transfer;
    /// std::mem::swap(&mut donor.velocity, &mut receiver.velocity);
    /// assert_eq!(actors.get(left).unwrap().energy, 8);
    /// assert_eq!(actors.get(right).unwrap().energy, 6);
    /// assert_eq!(actors.get(left).unwrap().velocity, -1);
    /// assert_eq!(actors.get(right).unwrap().velocity, 3);
    /// assert!(actors.is_full());
    /// # Ok::<(), PoolAccessError>(())
    /// ```
    pub fn get2_mut(
        &mut self,
        first: Handle,
        second: Handle,
    ) -> Result<(&mut T, &mut T), PoolAccessError> {
        let [first, second] = self.get_many_mut([first, second])?;
        Ok((first, second))
    }
    /// Borrow a fixed selection of live values mutably in caller order.
    ///
    /// All handles must belong to this pool. Before returning any references,
    /// checks every handle for a live matching generation in caller order,
    /// then rejects duplicate slots. [`PoolAccessError`] describes which error
    /// wins when a selection has multiple problems. An empty selection succeeds,
    /// including on a zero-capacity pool. Failure leaves all values, generations,
    /// and storage unchanged. Mutations through successful references are ordinary
    /// Rust mutations; they are not rolled back if subsequent caller code panics.
    ///
    /// Uses safe disjoint slice access with O(N²) validation work and O(N) stack
    /// storage, independent of pool capacity. Neither pool storage nor a returned
    /// collection allocates; values stay in their slots and handles stay valid.
    /// Prefer small selections; use [`Self::iter_mut`] to visit the whole pool.
    ///
    /// ```
    /// use ferravex_engine::pool::{Pool, PoolAccessError};
    ///
    /// let mut pool = Pool::new(3);
    /// let a = pool.insert(10).unwrap();
    /// let b = pool.insert(20).unwrap();
    /// let c = pool.insert(30).unwrap();
    /// let [last, first, middle] = pool.get_many_mut([c, a, b])?;
    /// *last += 3;
    /// *first += 1;
    /// *middle += 2;
    /// assert_eq!(pool.get(a), Some(&11));
    /// assert_eq!(pool.get(b), Some(&22));
    /// assert_eq!(pool.get(c), Some(&33));
    /// # Ok::<(), PoolAccessError>(())
    /// ```
    pub fn get_many_mut<const N: usize>(
        &mut self,
        handles: [Handle; N],
    ) -> Result<[&mut T; N], PoolAccessError> {
        for (position, &handle) in handles.iter().enumerate() {
            if self.get(handle).is_none() {
                return Err(PoolAccessError::InvalidHandle { position });
            }
        }
        for second in 0..N {
            for first in 0..second {
                if handles[first].index == handles[second].index {
                    return Err(PoolAccessError::DuplicateHandle { first, second });
                }
            }
        }
        let slots = self
            .slots
            .get_disjoint_mut(handles.map(|handle| handle.index))
            .expect("validated distinct live slot indices");
        Ok(slots.map(|slot| slot.value.as_mut().expect("validated live slot")))
    }
    pub fn remove(&mut self, h: Handle) -> Option<T> {
        let s = self.slots.get_mut(h.index)?;
        if s.generation != h.generation {
            return None;
        }
        let value = s.value.take()?;
        self.len -= 1;
        if let Some(next) = s.generation.checked_add(1) {
            s.generation = next;
            self.free.push(h.index);
        }
        Some(value)
    }
    /// Visit live handles and values in ascending slot order without allocating.
    ///
    /// Slot order can differ from insertion order after reuse.
    pub fn iter(&self) -> impl Iterator<Item = (Handle, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, s)| {
            s.value.as_ref().map(|v| {
                (
                    Handle {
                        index,
                        generation: s.generation,
                    },
                    v,
                )
            })
        })
    }
    /// Mutably visit live values in the same ascending slot order as [`Self::iter`].
    ///
    /// No pool storage is allocated. Updating a value preserves its handle.
    ///
    /// ```
    /// use ferravex_engine::pool::Pool;
    ///
    /// let mut pool = Pool::new(2);
    /// let handle = pool.insert(3).unwrap();
    /// for (current, value) in pool.iter_mut() {
    ///     assert_eq!(current, handle);
    ///     *value += 2;
    /// }
    /// assert_eq!(pool.get(handle), Some(&5));
    /// ```
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle, &mut T)> {
        self.slots.iter_mut().enumerate().filter_map(|(index, s)| {
            let handle = Handle {
                index,
                generation: s.generation,
            };
            s.value.as_mut().map(|value| (handle, value))
        })
    }
    /// Update live values and retain only those for which `keep` returns `true`.
    ///
    /// Visits each live slot once, in [`Self::iter`] order. Kept values preserve
    /// their handles; removed values are dropped and their handles invalidated
    /// just as with [`Self::remove`]. Retired and vacant slots are skipped.
    /// No temporary collection is created; the reusable-index vector can grow
    /// as in `remove`, and retains its allocation for subsequent reuse.
    ///
    /// If the predicate panics, earlier mutations and removals remain applied.
    /// The current value remains live (including any mutation before the panic),
    /// and later slots are untouched. If a removed value's destructor panics,
    /// its slot has already been removed and made reusable or retired.
    ///
    /// ```
    /// use ferravex_engine::pool::Pool;
    ///
    /// let mut lifetimes = Pool::new(2);
    /// let expired = lifetimes.insert(1_u32).unwrap();
    /// let active = lifetimes.insert(3_u32).unwrap();
    /// lifetimes.retain(|_, ticks| {
    ///     *ticks = ticks.saturating_sub(1);
    ///     *ticks > 0
    /// });
    /// assert_eq!(lifetimes.get(expired), None);
    /// assert_eq!(lifetimes.get(active), Some(&2));
    /// let reused = lifetimes.insert(5).unwrap();
    /// assert_ne!(expired, reused);
    /// ```
    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(Handle, &mut T) -> bool,
    {
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            let handle = Handle {
                index,
                generation: slot.generation,
            };
            if let Some(value) = slot.value.as_mut()
                && !keep(handle, value)
            {
                drop(self.remove(handle));
            }
        }
    }
    /// Drop all live values while retaining slot generations and backing storage.
    ///
    /// Old handles remain invalid after reinsertion. Exhausted slots stay retired;
    /// clearing does not restore their availability. This is equivalent to
    /// `retain(|_, _| false)`, including its allocation and destructor-panic behavior.
    ///
    /// ```
    /// use ferravex_engine::pool::Pool;
    ///
    /// let mut pool = Pool::new(1);
    /// let old = pool.insert("before restart").unwrap();
    /// pool.clear();
    /// assert!(pool.is_empty());
    /// assert_eq!(pool.available(), 1);
    /// let new = pool.insert("after restart").unwrap();
    /// assert_ne!(old, new);
    /// assert_eq!(pool.get(old), None);
    /// ```
    pub fn clear(&mut self) {
        self.retain(|_, _| false);
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_pair_preserves_requested_order_and_full_pool_handles() {
        let mut p = Pool::new(2);
        let a = p.insert(12).unwrap();
        let b = p.insert(2).unwrap();
        let (donor, receiver) = p.get2_mut(a, b).unwrap();
        *donor -= 4;
        *receiver += 4;
        let (receiver, donor) = p.get2_mut(b, a).unwrap();
        *receiver -= 1;
        *donor += 1;
        assert_eq!(p.get(a), Some(&9));
        assert_eq!(p.get(b), Some(&5));
        assert_eq!(p.len(), 2);
        assert_eq!(p.available(), 0);
        assert_eq!(p.insert(99), Err(99));
        assert_eq!(p.iter().map(|(h, _)| h).collect::<Vec<_>>(), [a, b]);
    }

    #[test]
    fn selected_many_preserves_every_three_slot_permutation() {
        let mut p = Pool::new(3);
        let handles = std::array::from_fn::<_, 3, _>(|i| p.insert(i).unwrap());
        for permutation in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let selection = permutation.map(|i| handles[i]);
            let values = p.get_many_mut(selection).unwrap();
            for (position, value) in values.into_iter().enumerate() {
                *value = 10 + position;
            }
            for (position, &handle) in selection.iter().enumerate() {
                assert_eq!(p.get(handle), Some(&(10 + position)));
            }
        }
    }

    #[test]
    fn selected_errors_validate_liveness_before_duplicates_without_mutation() {
        let mut p = Pool::new(4);
        let a = p.insert(10).unwrap();
        let stale = p.insert(20).unwrap();
        let vacant = p.insert(30).unwrap();
        p.remove(stale);
        let current = p.insert(40).unwrap();
        p.remove(vacant);
        let out_of_range = Handle {
            index: usize::MAX,
            generation: 0,
        };
        let vacant_generation = Handle {
            generation: vacant.generation + 1,
            ..vacant
        };
        for invalid in [stale, vacant, vacant_generation, out_of_range] {
            assert_eq!(
                p.get_many_mut([a, current, invalid]),
                Err(PoolAccessError::InvalidHandle { position: 2 })
            );
            assert_eq!(
                p.get_many_mut([invalid, a, invalid]),
                Err(PoolAccessError::InvalidHandle { position: 0 })
            );
            // Even a duplicate encountered earlier loses to an invalid handle.
            assert_eq!(
                p.get_many_mut([a, a, invalid]),
                Err(PoolAccessError::InvalidHandle { position: 2 })
            );
        }
        // Different generations at the same slot cannot bypass stale validation.
        assert_eq!(
            p.get2_mut(current, stale),
            Err(PoolAccessError::InvalidHandle { position: 1 })
        );
        assert_eq!(
            p.get2_mut(stale, current),
            Err(PoolAccessError::InvalidHandle { position: 0 })
        );
        assert_eq!(
            p.get2_mut(a, a),
            Err(PoolAccessError::DuplicateHandle {
                first: 0,
                second: 1
            })
        );
        assert_eq!(
            p.get_many_mut([a, current, current, a]),
            Err(PoolAccessError::DuplicateHandle {
                first: 1,
                second: 2
            })
        );
        assert_eq!(
            p.get_many_mut([a, current, a, a]),
            Err(PoolAccessError::DuplicateHandle {
                first: 0,
                second: 2
            })
        );
        assert_eq!(p.len(), 2);
        assert_eq!(p.available(), 2);
        assert_eq!(p.get(a), Some(&10));
        assert_eq!(p.get(current), Some(&40));
        let reused = p.insert(50).unwrap();
        assert_eq!(reused.index, vacant.index);
        assert_eq!(reused.generation, vacant_generation.generation);
    }

    #[test]
    fn selected_empty_and_single_work_at_zero_capacity_and_after_restart() {
        let mut empty = Pool::<()>::new(0);
        assert_eq!(empty.get_many_mut([]).unwrap().len(), 0);
        let invalid = Handle {
            index: 0,
            generation: 0,
        };
        assert_eq!(
            empty.get_many_mut([invalid]),
            Err(PoolAccessError::InvalidHandle { position: 0 })
        );
        assert_eq!(empty.slots.capacity(), 0);
        assert_eq!(empty.free.capacity(), 0);

        let mut p = Pool::new(2);
        let a = p.insert(()).unwrap();
        let b = p.insert(()).unwrap();
        assert_eq!(p.get_many_mut([b, a]).unwrap().len(), 2);
        p.clear();
        assert_eq!(p.get_many_mut([]).unwrap().len(), 0);
        assert_eq!(
            p.get2_mut(a, b),
            Err(PoolAccessError::InvalidHandle { position: 0 })
        );
        let c = p.insert(()).unwrap();
        let d = p.insert(()).unwrap();
        assert_eq!(p.get_many_mut([c]).unwrap().len(), 1);
        assert_eq!(p.get_many_mut([d, c]).unwrap().len(), 2);
        assert_eq!(
            p.get_many_mut([c, b]),
            Err(PoolAccessError::InvalidHandle { position: 1 })
        );
        assert_eq!(p.len(), 2);
        assert!(p.is_full());
    }

    #[test]
    fn selected_access_supports_last_generation_but_rejects_retired_slots() {
        let mut p = Pool::new(2);
        let a = p.insert(10).unwrap();
        let b = p.insert(20).unwrap();
        p.slots[a.index].generation = u64::MAX;
        let last = Handle {
            generation: u64::MAX,
            ..a
        };
        let (left, right) = p.get2_mut(last, b).unwrap();
        std::mem::swap(left, right);
        assert_eq!(p.remove(last), Some(20));
        assert_eq!(
            p.get_many_mut([b, last]),
            Err(PoolAccessError::InvalidHandle { position: 1 })
        );
        p.clear();
        assert_eq!(p.available(), 1);
        let reused = p.insert(30).unwrap();
        let [value] = p.get_many_mut([reused]).unwrap();
        *value += 1;
        assert_eq!(p.get(reused), Some(&31));
        assert_eq!(
            p.get2_mut(last, reused),
            Err(PoolAccessError::InvalidHandle { position: 0 })
        );
        assert_eq!(p.get_many_mut([]).unwrap().len(), 0);
        assert!(p.is_full());
        assert_eq!(p.insert(40), Err(40));
    }

    #[test]
    fn selecting_full_pool_does_not_grow_or_reorder_storage() {
        let mut p = Pool::new(64);
        let handles = std::array::from_fn::<_, 64, _>(|i| p.insert(i).unwrap());
        // Warm only reusable-index storage, then return to a full pool.
        let removed = handles[20];
        p.remove(removed);
        let mut handles = handles;
        handles[20] = p.insert(20).unwrap();
        let storage = (
            p.slots.as_ptr(),
            p.slots.len(),
            p.slots.capacity(),
            p.free.as_ptr(),
            p.free.len(),
            p.free.capacity(),
        );
        let mut reversed = handles;
        reversed.reverse();
        for (i, value) in p.get_many_mut(reversed).unwrap().into_iter().enumerate() {
            *value = i;
        }
        assert_eq!(
            p.get_many_mut([handles[0], removed]),
            Err(PoolAccessError::InvalidHandle { position: 1 })
        );
        assert_eq!(
            p.get2_mut(handles[0], handles[0]),
            Err(PoolAccessError::DuplicateHandle {
                first: 0,
                second: 1
            })
        );
        assert_eq!(
            (
                p.slots.as_ptr(),
                p.slots.len(),
                p.slots.capacity(),
                p.free.as_ptr(),
                p.free.len(),
                p.free.capacity()
            ),
            storage
        );
        assert_eq!(p.len(), 64);
        assert!(p.is_full());
        for (i, &handle) in handles.iter().enumerate() {
            assert_eq!(p.get(handle), Some(&(63 - i)));
        }
    }

    #[test]
    fn reuse_rejects_stale_handles_and_preserves_live_objects() {
        let mut p = Pool::new(2);
        let a = p.insert("a").unwrap();
        let b = p.insert("b").unwrap();
        assert_eq!(p.insert("full"), Err("full"));
        assert_eq!(p.remove(a), Some("a"));
        let c = p.insert("c").unwrap();
        assert_ne!(a, c);
        assert_eq!(a.index, c.index);
        assert_eq!(p.get(a), None);
        assert_eq!(p.remove(a), None);
        *p.get_mut(c).unwrap() = "updated";
        assert_eq!(
            p.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
            ["updated", "b"]
        );
        assert_eq!(p.get(b), Some(&"b"));
        p.remove(b);
        p.remove(c);
        assert!(p.is_empty());
    }
    #[test]
    fn generation_exhaustion_retires_slot() {
        let mut p = Pool::new(1);
        let h = p.insert(1).unwrap();
        p.slots[h.index].generation = u64::MAX;
        let h = Handle {
            generation: u64::MAX,
            ..h
        };
        assert_eq!(p.remove(h), Some(1));
        assert_eq!(p.insert(2), Err(2));
        assert!(p.get(h).is_none());
    }

    #[test]
    fn capacity_and_availability_follow_successful_operations() {
        let mut p = Pool::new(2);
        assert_eq!(p.capacity(), 2);
        assert_eq!(p.available(), 2);
        assert!(!p.is_full());
        let a = p.insert(1).unwrap();
        assert_eq!(p.available(), 1);
        let b = p.insert(2).unwrap();
        assert!(p.is_full());
        assert_eq!(p.insert(3), Err(3));
        assert_eq!(p.available(), 0);
        assert_eq!(p.remove(a), Some(1));
        assert_eq!(p.available(), 1);
        assert_eq!(p.remove(a), None);
        assert_eq!(p.available(), 1);
        let reused = p.insert(3).unwrap();
        assert!(p.is_full());
        assert_eq!(p.get_mut(a), None);
        assert_eq!(p.remove(a), None);
        assert_eq!(p.len(), 2);
        assert_eq!(p.get(reused), Some(&3));
        assert_eq!(p.get(b), Some(&2));
        assert_eq!(p.capacity(), 2);
    }

    #[test]
    fn zero_and_maximum_capacity_remain_lazy_and_checked() {
        let mut empty = Pool::<u8>::new(0);
        assert_eq!(empty.capacity(), 0);
        assert_eq!(empty.available(), 0);
        assert!(empty.is_empty());
        assert!(empty.is_full());
        assert_eq!(empty.insert(7), Err(7));
        assert_eq!(empty.iter_mut().count(), 0);
        empty.retain(|_, _| panic!("empty pools have no values to visit"));
        empty.clear();
        assert_eq!(empty.slots.capacity(), 0);
        assert_eq!(empty.free.capacity(), 0);

        let large = Pool::<u8>::new(usize::MAX);
        assert_eq!(large.capacity(), usize::MAX);
        assert_eq!(large.available(), usize::MAX);
        assert!(!large.is_full());
        assert_eq!(large.slots.capacity(), 0);
        assert_eq!(large.free.capacity(), 0);
    }

    #[test]
    fn retired_slots_are_unavailable_with_reusable_and_uncreated_slots() {
        let mut p = Pool::new(3);
        let a = p.insert(1).unwrap();
        let b = p.insert(2).unwrap();
        p.slots[a.index].generation = u64::MAX - 1;
        let penultimate = Handle {
            generation: u64::MAX - 1,
            ..a
        };
        assert_eq!(p.remove(penultimate), Some(1));
        let last = p.insert(3).unwrap();
        assert_eq!(last.generation, u64::MAX);
        p.retain(|h, _| h != last);
        assert_eq!(p.capacity(), 3);
        assert_eq!(p.len(), 1);
        assert_eq!(p.available(), 1);
        assert_eq!(p.get(last), None);
        assert_eq!(p.remove(last), None);
        assert_eq!(p.remove(b), Some(2));
        assert_eq!(p.available(), 2);
        let reused = p.insert(4).unwrap();
        let created = p.insert(5).unwrap();
        assert_eq!(reused.index, b.index);
        assert_eq!(created.index, 2);
        assert!(p.is_full());
        assert_eq!(p.insert(6), Err(6));
        p.clear();
        assert_eq!(p.available(), 2);
        assert_eq!(p.get_mut(last), None);
        p.retain(|_, _| panic!("vacant and retired slots must be skipped"));
    }

    #[test]
    fn clearing_final_generation_leaves_an_empty_full_pool() {
        let mut p = Pool::new(1);
        let h = p.insert(7).unwrap();
        p.slots[h.index].generation = u64::MAX;
        let last = Handle {
            generation: u64::MAX,
            ..h
        };
        p.clear();
        p.clear();
        assert!(p.is_empty());
        assert!(p.is_full());
        assert_eq!(p.capacity(), 1);
        assert_eq!(p.available(), 0);
        assert_eq!(p.get(last), None);
        assert_eq!(p.insert(8), Err(8));
    }

    #[test]
    fn mutable_iteration_uses_slot_order_and_preserves_live_handles() {
        let mut p = Pool::new(5);
        let a = p.insert(1).unwrap();
        let b = p.insert(2).unwrap();
        let c = p.insert(3).unwrap();
        let d = p.insert(4).unwrap();
        p.slots[d.index].generation = u64::MAX;
        let retired = Handle {
            generation: u64::MAX,
            ..d
        };
        p.remove(retired);
        p.remove(b);
        let reused = p.insert(5).unwrap();
        let mut visited = Vec::new();
        for (handle, value) in p.iter_mut() {
            visited.push(handle);
            *value *= 10;
        }
        assert_eq!(visited, [a, reused, c]);
        assert_eq!(p.get(a), Some(&10));
        assert_eq!(p.get(reused), Some(&50));
        assert_eq!(p.get(c), Some(&30));
        assert_eq!(p.get(b), None);
        assert_eq!(p.get(retired), None);
        assert_eq!(p.len(), 3);
        assert_eq!(p.available(), 1);
        assert_eq!(p.iter().map(|(h, _)| h).collect::<Vec<_>>(), visited);
    }

    #[test]
    fn retain_updates_and_selects_by_handle_without_invalidating_survivors() {
        let mut p = Pool::new(5);
        let a = p.insert(1).unwrap();
        let b = p.insert(2).unwrap();
        let c = p.insert(3).unwrap();
        let d = p.insert(4).unwrap();
        p.remove(b);
        let mut visited = Vec::new();
        p.retain(|h, value| {
            visited.push(h);
            *value += 10;
            h == a || h == d
        });
        assert_eq!(visited, [a, c, d]);
        assert_eq!(
            p.iter().map(|(h, v)| (h, *v)).collect::<Vec<_>>(),
            [(a, 11), (d, 14)]
        );
        assert_eq!(p.len(), 2);
        assert_eq!(p.available(), 3);
        assert_eq!(p.get(c), None);
        let reused_c = p.insert(30).unwrap();
        let reused_b = p.insert(20).unwrap();
        let created = p.insert(50).unwrap();
        assert_eq!(reused_c.index, c.index);
        assert_eq!(reused_b.index, b.index);
        assert_ne!(reused_c, c);
        assert_ne!(reused_b, b);
        assert_eq!(created.index, 4);
        assert!(p.is_full());
        p.retain(|_, _| true);
        assert_eq!(p.len(), 5);
        p.retain(|_, _| false);
        assert!(p.is_empty());
        assert_eq!(p.available(), 5);
    }

    #[test]
    fn repeated_clear_reuses_storage_without_reviving_stale_handles() {
        let mut p = Pool::new(3);
        let a = p.insert(()).unwrap();
        let b = p.insert(()).unwrap();
        let c = p.insert(()).unwrap();
        p.remove(b);
        p.clear();
        let slots_capacity = p.slots.capacity();
        let free_capacity = p.free.capacity();
        let mut old = vec![a, b, c];
        for _ in 0..8 {
            p.clear();
            assert_eq!(p.available(), 3);
            let current: Vec<_> = (0..3).map(|_| p.insert(()).unwrap()).collect();
            assert!(p.is_full());
            for &h in &old {
                assert_eq!(p.get(h), None);
                assert_eq!(p.get_mut(h), None);
                assert_eq!(p.remove(h), None);
            }
            assert_eq!(p.len(), 3);
            assert_eq!(p.iter().count(), 3);
            assert_eq!(p.insert(()), Err(()));
            old.extend(current);
            p.clear();
            assert!(p.is_empty());
            assert_eq!(p.slots.capacity(), slots_capacity);
            assert_eq!(p.free.capacity(), free_capacity);
        }
    }

    #[test]
    fn predicate_panic_keeps_current_mutation_and_later_values_live() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let mut p = Pool::new(3);
        let a = p.insert(1).unwrap();
        let b = p.insert(2).unwrap();
        let c = p.insert(3).unwrap();
        let result = catch_unwind(AssertUnwindSafe(|| {
            p.retain(|h, value| {
                *value += 10;
                assert_ne!(h, b, "predicate panic after mutating the current value");
                false
            });
        }));
        assert!(result.is_err());
        assert_eq!(p.get(a), None);
        assert_eq!(p.get(b), Some(&12));
        assert_eq!(p.get(c), Some(&3));
        assert_eq!(p.len(), 2);
        assert_eq!(p.available(), 1);
        let reused = p.insert(4).unwrap();
        assert_eq!(reused.index, a.index);
        assert_ne!(reused, a);
        p.clear();
        assert_eq!(p.available(), 3);
    }

    #[test]
    fn destructor_panic_during_clear_leaves_removed_slots_reusable() {
        use std::{
            cell::Cell,
            panic::{AssertUnwindSafe, catch_unwind},
            rc::Rc,
        };

        #[derive(Debug)]
        struct Value {
            drops: Rc<Cell<usize>>,
            panic_on_drop: bool,
        }
        impl Drop for Value {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
                assert!(!self.panic_on_drop, "destructor panic");
            }
        }

        let drops = Rc::new(Cell::new(0));
        let mut p = Pool::new(3);
        let old: Vec<_> = [false, true, false]
            .into_iter()
            .map(|panic_on_drop| {
                p.insert(Value {
                    drops: drops.clone(),
                    panic_on_drop,
                })
                .unwrap()
            })
            .collect();
        assert!(catch_unwind(AssertUnwindSafe(|| p.clear())).is_err());
        assert_eq!(drops.get(), 2);
        assert_eq!(p.len(), 1);
        assert_eq!(p.available(), 2);
        assert!(p.get(old[0]).is_none());
        assert!(p.get(old[1]).is_none());
        assert!(p.get(old[2]).is_some());
        let reused = p
            .insert(Value {
                drops: drops.clone(),
                panic_on_drop: false,
            })
            .unwrap();
        assert_eq!(reused.index, old[1].index);
        assert_ne!(reused, old[1]);
        p.clear();
        p.clear();
        assert_eq!(drops.get(), 4);
        assert!(p.is_empty());
        assert_eq!(p.available(), 3);
        drop(p);
        assert_eq!(drops.get(), 4);
    }
}
