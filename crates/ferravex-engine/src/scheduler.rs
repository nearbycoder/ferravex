//! Bounded tick deadlines with deterministic FIFO ties and explicit work limits.
//! IDs belong to their scheduler; tick values are monotonic absolute host ticks.
//!
//! [`TickScheduler::drain_ready_iter`] processes tasks without allocating an output
//! collection. [`TickScheduler::drain_ready_into`] reuses a caller's buffer without
//! growing it. Both leave excess ready work queued. The tree may release internal
//! storage while draining; scheduling and rescheduling may allocate tree nodes.
//!
//! Owner scopes are opt-in via [`TickScheduler::with_scope_capacity`]. Cancelling
//! a scope invalidates its handle and removes only its queued jobs. A handle is
//! tied to its issuing scheduler even after that scheduler is moved or dropped.
//! Dropping a handle does not cancel anything; the host owns lifecycle decisions.
//! Deadlines use ordinary numeric `u64` order, not wrapping arithmetic. Cancel
//! old timeline scopes before resetting/wrapping the clock and discard retained
//! tick batches; explicitly handle any unscoped jobs on that timeline as well.
//!
//! # Delayed respawn on actual absolute ticks
//!
//! This headless recipe executes task 201 contexts, including a zero-tick frame,
//! catch-up, manual stepping and a round restart. A local multiplayer menu must
//! suppress commands rather than pause this authoritative session.
//!
//! ```
//! use ferravex_engine::{scheduler::TickScheduler, session::GameSession};
//!
//! let mut session = GameSession::new(64.0, 4, true);
//! let mut jobs = TickScheduler::with_scope_capacity(8, 1);
//! let round = jobs.create_scope().unwrap();
//! let mut alive = true;
//! let mut respawns = 0;
//! let mut executed = 0;
//! assert_eq!(session.advance_batch(0.5 / 64.0).ticks().count(), 0);
//! for context in session.advance_batch(3.0 / 64.0).ticks() {
//!     executed += 1;
//!     if context.tick == 1 {
//!         alive = false; // Accepted death event, not a render-frame request.
//!         let deadline = context.tick.checked_add(2).expect("restart before wrap");
//!         jobs.schedule_scoped(&round, deadline, "respawn").unwrap();
//!     }
//!     for event in jobs.drain_ready_iter(context.tick, 8) {
//!         assert_eq!((context.tick, event), (3, "respawn"));
//!         alive = true;
//!         respawns += 1; // Feedback can follow this accepted spawn.
//!     }
//! }
//! assert!(alive);
//! assert_eq!((executed, respawns), (3, 1));
//! session.set_paused(true);
//! jobs.schedule_scoped(&round, 4, "respawn").unwrap();
//! for context in session.step_paused_batch().unwrap().ticks() {
//!     assert_eq!(jobs.drain_ready(context.tick, 8), ["respawn"]);
//! }
//! jobs.schedule_scoped(&round, 100, "old round").unwrap();
//! assert_eq!(jobs.cancel_scope(&round), Some(1));
//! session.restart(); // Discard any retained batches and old tick deadlines.
//! let next_round = jobs.create_scope().unwrap();
//! assert_ne!(round, next_round);
//! assert_eq!(jobs.schedule_scoped(&round, 1, "stale"), Err("stale"));
//! assert_eq!(jobs.cancel_scope(&round), None);
//! jobs.schedule_scoped(&next_round, 1, "new round").unwrap();
//! for context in session.advance_batch(1.0 / 64.0).ticks() {
//!     assert_eq!(jobs.drain_ready(context.tick, 8), ["new round"]);
//! }
//! ```
use std::{collections::BTreeMap, iter::FusedIterator, ops::Bound, sync::Arc};

/// A task's identity and original insertion order within its scheduler.
///
/// IDs are never reused by that scheduler. Only use an ID with the scheduler that
/// issued it: IDs from different schedulers can have the same value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskId(u64);

/// A generation-checked lifecycle handle issued by one [`TickScheduler`].
///
/// Clones name the same scope. Foreign, cancelled and reused handles are rejected;
/// dropping the last handle does not release its slot. Explicitly cancel the
/// scope even after all its tasks have drained. Handles are not wire/save IDs.
/// See the module's runnable respawn recipe and [`TickScheduler::cancel_scope_with`].
#[derive(Clone, Debug)]
pub struct ScopeId {
    owner: Arc<()>,
    key: ScopeKey,
}

impl PartialEq for ScopeId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.key == other.key
    }
}
impl Eq for ScopeId {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScopeKey {
    slot: usize,
    generation: u64,
}

#[derive(Clone)]
struct ScopeSlot {
    generation: u64,
    active: bool,
}

struct ScheduledTask<T> {
    payload: T,
    scope: Option<ScopeKey>,
}

/// A bounded deadline queue ordered by tick, then original insertion order.
///
/// Capacity counts tasks, not payload bytes. Tick values are caller supplied;
/// scheduling or rescheduling into the past makes work ready on the next drain.
pub struct TickScheduler<T> {
    tasks: BTreeMap<(u64, u64), ScheduledTask<T>>,
    scopes: Vec<ScopeSlot>,
    scope_owner: Option<Arc<()>>,
    next: u64,
    capacity: usize,
}
impl<T> TickScheduler<T> {
    /// Creates an empty queue with a maximum task count. Zero rejects all tasks.
    pub fn new(capacity: usize) -> Self {
        Self {
            tasks: BTreeMap::new(),
            scopes: Vec::new(),
            scope_owner: None,
            next: 0,
            capacity,
        }
    }
    /// Creates a queue with independent task and live-scope count limits.
    ///
    /// Allocates fixed scope storage and one shared identity when `scope_capacity`
    /// is nonzero. Scope creation, cloning, validation and cancellation do not
    /// grow this storage. Payload bytes and caller-held handle clones are not
    /// bounded by these counts. Allocation failure follows ordinary std policy.
    /// [`Self::new`] remains unscoped and allocates no scope storage.
    /// See the module's runnable respawn recipe.
    pub fn with_scope_capacity(capacity: usize, scope_capacity: usize) -> Self {
        let mut scheduler = Self::new(capacity);
        scheduler.scopes = vec![
            ScopeSlot {
                generation: 0,
                active: false
            };
            scope_capacity
        ];
        scheduler.scope_owner = (scope_capacity != 0).then(|| Arc::new(()));
        scheduler
    }

    /// Acquires a scope slot, or returns `None` without mutation when none is free.
    ///
    /// Reuse increments its generation. Slots at generation `u64::MAX` retire
    /// after cancellation rather than wrap and revive stale handles. Empty scopes
    /// still occupy a slot; cancelling/draining individual jobs does not free it.
    /// Task capacity and task IDs are unaffected. See the module recipe.
    pub fn create_scope(&mut self) -> Option<ScopeId> {
        let owner = self.scope_owner.as_ref()?;
        let (slot, state) = self
            .scopes
            .iter_mut()
            .enumerate()
            .find(|(_, state)| !state.active && state.generation != u64::MAX)?;
        state.generation += 1;
        state.active = true;
        Some(ScopeId {
            owner: Arc::clone(owner),
            key: ScopeKey {
                slot,
                generation: state.generation,
            },
        })
    }

    /// Whether this scheduler still owns the live generation of `scope`.
    ///
    /// Foreign handles are rejected even if their slot and generation match.
    /// See the module recipe for stale-handle rejection across restart.
    pub fn is_scope_active(&self, scope: &ScopeId) -> bool {
        self.scope_owner
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, &scope.owner))
            && self
                .scopes
                .get(scope.key.slot)
                .is_some_and(|state| state.active && state.generation == scope.key.generation)
    }

    /// Schedules a job belonging to a live scope at an absolute tick.
    ///
    /// Returns the payload unchanged for a foreign/stale scope, full queue or
    /// exhausted task ID sequence, consuming no ID or capacity on rejection.
    /// Scoped and unscoped jobs share one capacity and original FIFO sequence.
    /// Rescheduling preserves membership; ready draining returns just the payload
    /// and never invokes it. See the module's runnable respawn recipe.
    pub fn schedule_scoped(&mut self, scope: &ScopeId, tick: u64, task: T) -> Result<TaskId, T> {
        if !self.is_scope_active(scope) {
            return Err(task);
        }
        self.schedule_inner(tick, task, Some(scope.key))
    }

    /// Invalidates a scope and discards all its queued payloads, returning a count.
    ///
    /// A live empty scope returns `Some(0)`. Foreign/stale handles return `None`
    /// without mutation. Uses [`Self::cancel_scope_with`] with `drop`; callable
    /// payloads are dropped, never executed. Its ordering and panic policy apply.
    /// See the module recipe for cancelling a round before resetting its clock.
    pub fn cancel_scope(&mut self, scope: &ScopeId) -> Option<usize> {
        self.cancel_scope_with(scope, |_, task| drop(task))
    }

    /// Invalidates a scope and passes its queued jobs to `cancel` in deadline/FIFO
    /// order, returning their count. Other scopes, unscoped jobs, deadlines and
    /// the task ID sequence stay unchanged. Allocates no collection or tree nodes.
    ///
    /// The callback owns each payload and can retain, return through caller
    /// storage, or discard it. The scope is invalidated before any callback runs.
    /// On callback/destructor panic with unwinding, the current job is already
    /// removed and all unvisited members are discarded by a guard, without more
    /// callbacks. The scope stays invalid. A second destructor panic during
    /// unwinding follows Rust's abort policy; panic=abort has no cleanup guarantee.
    /// Callback/destructor allocations and side effects are caller-owned.
    ///
    /// A foreign/stale handle returns `None` and never invokes the callback.
    /// Scanning is linear in queued jobs plus tree-removal costs. The scheduler
    /// stays exclusively borrowed for the call; callbacks cannot reenter it.
    ///
    /// ```
    /// use ferravex_engine::scheduler::TickScheduler;
    /// let mut jobs = TickScheduler::with_scope_capacity(3, 1);
    /// let scope = jobs.create_scope().unwrap();
    /// assert!(jobs.is_scope_active(&scope));
    /// let id = jobs.schedule_scoped(&scope, 10, "respawn").unwrap();
    /// let mut returned = Vec::with_capacity(1);
    /// assert_eq!(jobs.cancel_scope_with(&scope, |id, job| returned.push((id, job))), Some(1));
    /// assert_eq!(returned, [(id, "respawn")]);
    /// assert!(!jobs.is_scope_active(&scope));
    /// assert_eq!(jobs.cancel_scope_with(&scope, |_, _| unreachable!()), None);
    /// ```
    ///
    /// ```compile_fail
    /// use ferravex_engine::scheduler::TickScheduler;
    /// let mut jobs = TickScheduler::<()>::with_scope_capacity(1, 1);
    /// let scope = jobs.create_scope().unwrap();
    /// jobs.cancel_scope_with(&scope, |_, _| { jobs.create_scope(); });
    /// ```
    pub fn cancel_scope_with(
        &mut self,
        scope: &ScopeId,
        mut cancel: impl FnMut(TaskId, T),
    ) -> Option<usize> {
        if !self.is_scope_active(scope) {
            return None;
        }
        self.scopes[scope.key.slot].active = false;
        let mut cancelled = ScopeCancellation {
            tasks: &mut self.tasks,
            scope: scope.key,
            after: None,
        };
        let mut count = 0;
        for (id, task) in &mut cancelled {
            count += 1;
            cancel(id, task);
        }
        Some(count)
    }

    /// Schedules a task at an absolute tick, returning its stable ID.
    ///
    /// Returns the task unchanged if capacity or the ID sequence is exhausted.
    /// Equal deadlines run in original insertion order, even after rescheduling.
    pub fn schedule(&mut self, tick: u64, task: T) -> Result<TaskId, T> {
        self.schedule_inner(tick, task, None)
    }

    fn schedule_inner(&mut self, tick: u64, task: T, scope: Option<ScopeKey>) -> Result<TaskId, T> {
        if self.tasks.len() >= self.capacity || self.next == u64::MAX {
            return Err(task);
        }
        let id = self.next;
        self.next += 1;
        self.tasks.insert(
            (tick, id),
            ScheduledTask {
                payload: task,
                scope,
            },
        );
        Ok(TaskId(id))
    }
    /// Removes a task, returning its payload, or `None` if it is no longer queued.
    pub fn cancel(&mut self, id: TaskId) -> Option<T> {
        let key = self.task_key(id)?;
        self.tasks.remove(&key).map(|task| task.payload)
    }

    /// Changes a queued task's absolute deadline and returns the previous tick.
    ///
    /// An absent, cancelled or drained ID returns `None` without changing the
    /// queue. Every `u64` tick is valid, including past ticks and `u64::MAX`.
    /// Rescheduling to the same tick is a successful no-op.
    ///
    /// The task keeps its ID and original insertion order among equal deadlines.
    /// No capacity or new ID is consumed, so this works even when the queue is
    /// full or the ID sequence is exhausted. Like cancellation, finding the ID
    /// takes a linear scan; changing the deadline may allocate tree nodes.
    ///
    /// ```
    /// use ferravex_engine::scheduler::TickScheduler;
    ///
    /// let mut tasks = TickScheduler::new(2);
    /// let first = tasks.schedule(20, "first").unwrap();
    /// tasks.schedule(10, "second").unwrap();
    /// assert_eq!(tasks.reschedule(first, 10), Some(20));
    /// assert_eq!(tasks.drain_ready(10, 2), ["first", "second"]);
    /// assert_eq!(tasks.reschedule(first, 30), None);
    /// ```
    pub fn reschedule(&mut self, id: TaskId, tick: u64) -> Option<u64> {
        let key = self.task_key(id)?;
        if key.0 != tick {
            let task = self.tasks.remove(&key)?;
            self.tasks.insert((tick, id.0), task);
        }
        Some(key.0)
    }

    fn task_key(&self, id: TaskId) -> Option<(u64, u64)> {
        self.tasks
            .keys()
            .find(|(_, serial)| *serial == id.0)
            .copied()
    }

    /// Earliest queued deadline, including work left over from a limited drain.
    pub fn next_tick(&self) -> Option<u64> {
        self.tasks.first_key_value().map(|((tick, _), _)| *tick)
    }

    /// Collects at most `limit` due tasks; excess work remains for a later call.
    ///
    /// This convenience method collects into a fresh vector, which may allocate.
    /// Use [`Self::drain_ready_iter`] or [`Self::drain_ready_into`] to avoid that
    /// allocation. All draining methods preserve deadline and FIFO tie order.
    pub fn drain_ready(&mut self, tick: u64, limit: usize) -> Vec<T> {
        self.drain_ready_iter(tick, limit).collect()
    }

    /// Lazily removes at most `limit` tasks with deadlines at or before `tick`.
    ///
    /// The iterator neither allocates nor clones payloads. Only yielded tasks
    /// leave the queue: dropping it early leaves all unyielded tasks available
    /// for a later drain, cancellation or rescheduling. A zero limit removes
    /// nothing. The scheduler stays mutably borrowed until the iterator is
    /// dropped, and the iterator remains exhausted after returning `None`.
    ///
    /// ```
    /// use ferravex_engine::scheduler::TickScheduler;
    ///
    /// let mut tasks = TickScheduler::new(3);
    /// tasks.schedule(4, "first").unwrap();
    /// let second = tasks.schedule(4, "second").unwrap();
    /// tasks.schedule(8, "later").unwrap();
    /// {
    ///     let mut ready = tasks.drain_ready_iter(4, 2);
    ///     assert_eq!(ready.next(), Some("first"));
    /// } // The unyielded second task is still queued.
    /// assert_eq!(tasks.cancel(second), Some("second"));
    /// assert_eq!(tasks.next_tick(), Some(8));
    /// ```
    pub fn drain_ready_iter(
        &mut self,
        tick: u64,
        limit: usize,
    ) -> impl FusedIterator<Item = T> + '_ {
        std::iter::from_fn(move || {
            if self.next_tick().is_some_and(|next| next <= tick) {
                self.tasks.pop_first().map(|(_, task)| task.payload)
            } else {
                None
            }
        })
        .take(limit)
        .fuse()
    }

    /// Appends ready tasks to `out` without allocating or growing its capacity.
    ///
    /// Removes at most `limit` tasks and at most `out.capacity() - out.len()`
    /// tasks, returning the number appended. Existing output entries are kept.
    /// Ready work that does not fit remains queued, including when the buffer
    /// has no spare capacity. Clear or consume the buffer between ticks to reuse
    /// its storage. Tasks are moved without cloning.
    ///
    /// ```
    /// use ferravex_engine::scheduler::TickScheduler;
    ///
    /// let mut tasks = TickScheduler::new(3);
    /// tasks.schedule(4, "first").unwrap();
    /// tasks.schedule(4, "second").unwrap();
    /// tasks.schedule(8, "later").unwrap();
    /// let mut ready = Vec::with_capacity(2);
    /// ready.push("already collected");
    /// assert_eq!(tasks.drain_ready_into(4, 1, &mut ready), 1);
    /// assert_eq!(ready, ["already collected", "first"]);
    /// ready.clear();
    /// assert_eq!(tasks.drain_ready_into(4, 2, &mut ready), 1);
    /// assert_eq!(ready, ["second"]);
    /// assert_eq!(tasks.next_tick(), Some(8));
    /// ```
    pub fn drain_ready_into(&mut self, tick: u64, limit: usize, out: &mut Vec<T>) -> usize {
        let limit = limit.min(out.capacity() - out.len());
        let before = out.len();
        for task in self.drain_ready_iter(tick, limit) {
            out.push(task);
        }
        out.len() - before
    }

    /// Number of queued tasks, including overdue work.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }
    /// Whether no tasks are queued.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}
// Invalidating the slot happens before this exclusive task-map borrow. Drop
// finishes cancellation if caller code unwinds; no user callback runs in Drop.
struct ScopeCancellation<'a, T> {
    tasks: &'a mut BTreeMap<(u64, u64), ScheduledTask<T>>,
    scope: ScopeKey,
    after: Option<(u64, u64)>,
}

impl<T> Iterator for ScopeCancellation<'_, T> {
    type Item = (TaskId, T);

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.after.map_or(Bound::Unbounded, Bound::Excluded);
        let key = self
            .tasks
            .range((start, Bound::Unbounded))
            .find(|(_, task)| task.scope == Some(self.scope))
            .map(|(key, _)| *key)?;
        self.after = Some(key);
        self.tasks
            .remove(&key)
            .map(|task| (TaskId(key.1), task.payload))
    }
}

impl<T> Drop for ScopeCancellation<'_, T> {
    fn drop(&mut self) {
        self.tasks.retain(|_, task| task.scope != Some(self.scope));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_capacity_empty_release_clones_and_foreign_provenance() {
        let mut legacy = TickScheduler::<()>::new(3);
        assert!(legacy.create_scope().is_none());
        let mut zero = TickScheduler::<()>::with_scope_capacity(0, 0);
        assert!(zero.create_scope().is_none());
        let mut q = TickScheduler::<()>::with_scope_capacity(0, 1);
        let scope = q.create_scope().unwrap();
        let clone = scope.clone();
        assert_eq!(scope, clone);
        assert!(q.create_scope().is_none());
        assert_eq!(q.schedule_scoped(&scope, 0, ()), Err(()));
        let mut other = TickScheduler::with_scope_capacity(1, 1);
        let foreign = other.create_scope().unwrap();
        assert_eq!(scope.key, foreign.key);
        assert_ne!(scope, foreign);
        assert!(!q.is_scope_active(&foreign));
        assert_eq!(q.schedule_scoped(&foreign, 0, ()), Err(()));
        assert_eq!(
            q.cancel_scope_with(&foreign, |_, _| panic!("foreign")),
            None
        );
        assert_eq!(q.cancel_scope(&scope), Some(0));
        assert!(!q.is_scope_active(&clone));
        assert_eq!(q.cancel_scope(&clone), None);
        let replacement = q.create_scope().unwrap();
        assert_ne!(scope, replacement);
        assert_eq!(q.cancel_scope(&clone), None);
        assert!(q.is_scope_active(&replacement));
        drop(q);
        assert_eq!(other.schedule_scoped(&scope, 0, ()), Err(()));
        assert!(other.is_scope_active(&foreign));
    }

    #[test]
    fn moving_scheduler_preserves_identity_and_dropping_handles_does_not_cancel() {
        let mut q = TickScheduler::with_scope_capacity(2, 1);
        let scope = q.create_scope().unwrap();
        q.schedule_scoped(&scope, 9, "queued").unwrap();
        let mut moved = q;
        assert!(moved.is_scope_active(&scope));
        drop(scope);
        assert!(moved.create_scope().is_none());
        assert_eq!(moved.drain_ready(9, 2), ["queued"]);
        assert!(moved.create_scope().is_none());
    }

    #[test]
    fn scope_cancellation_preserves_unrelated_deadlines_ids_and_rescheduled_members() {
        let mut q = TickScheduler::with_scope_capacity(7, 2);
        let a = q.create_scope().unwrap();
        let b = q.create_scope().unwrap();
        let first = q.schedule_scoped(&a, 90, "a first").unwrap();
        let plain = q.schedule(5, "plain").unwrap();
        let second = q.schedule_scoped(&a, 5, "a second").unwrap();
        let other = q.schedule_scoped(&b, 5, "b").unwrap();
        let early = q.schedule_scoped(&a, 1, "a early").unwrap();
        q.schedule_scoped(&a, u64::MAX, "a last").unwrap();
        assert_eq!(q.reschedule(first, 5), Some(90));
        assert_eq!(q.reschedule(second, 5), Some(5));
        let mut removed = Vec::new();
        assert_eq!(
            q.cancel_scope_with(&a, |id, job| removed.push((id, job))),
            Some(4)
        );
        assert_eq!(
            removed.iter().map(|(_, job)| *job).collect::<Vec<_>>(),
            ["a early", "a first", "a second", "a last"]
        );
        assert_eq!(
            (removed[0].0, removed[1].0, removed[2].0),
            (early, first, second)
        );
        assert_eq!(q.next_tick(), Some(5));
        assert_eq!(q.len(), 2);
        assert_eq!(q.cancel(first), None);
        assert_eq!(q.reschedule(second, 1), None);
        let next = q.schedule(5, "new").unwrap();
        assert_eq!(next.0, 6);
        assert_eq!(q.reschedule(other, 6), Some(5));
        assert_eq!(q.reschedule(other, 5), Some(6));
        assert_eq!(q.drain_ready(5, 7), ["plain", "b", "new"]);
        assert_eq!(q.cancel(plain), None);
        assert!(q.is_scope_active(&b));
        assert_eq!(q.cancel_scope(&b), Some(0));
    }

    #[test]
    fn scoped_ready_draining_uses_global_original_fifo_and_keeps_scope_live() {
        let mut q = TickScheduler::with_scope_capacity(4, 1);
        let scope = q.create_scope().unwrap();
        let first = q.schedule_scoped(&scope, 9, "first").unwrap();
        q.schedule(3, "plain").unwrap();
        let third = q.schedule_scoped(&scope, 3, "third").unwrap();
        assert_eq!(q.reschedule(first, 3), Some(9));
        {
            assert_eq!(q.drain_ready_iter(3, 4).next(), Some("first"));
        }
        assert_eq!(q.drain_ready(3, 1), ["plain"]);
        assert_eq!(q.cancel(third), Some("third"));
        assert!(q.create_scope().is_none());
        assert!(q.is_scope_active(&scope));
        assert_eq!(q.cancel_scope(&scope), Some(0));
        let next = q.create_scope().unwrap();
        assert_eq!(q.schedule_scoped(&scope, 0, "stale"), Err("stale"));
        assert_eq!(q.cancel_scope(&scope), None);
        assert_eq!(q.schedule_scoped(&next, 0, "next").unwrap().0, 3);
        assert_eq!(q.drain_ready(0, 1), ["next"]);
    }

    #[test]
    fn scope_rejections_preserve_payloads_capacity_and_exhausted_task_sequence() {
        use std::{cell::Cell, rc::Rc};
        #[derive(Debug)]
        struct Payload(Rc<Cell<usize>>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut q = TickScheduler::with_scope_capacity(1, 1);
        let scope = q.create_scope().unwrap();
        let first = q
            .schedule_scoped(&scope, 9, Payload(drops.clone()))
            .unwrap();
        let rejected = q
            .schedule_scoped(&scope, 0, Payload(drops.clone()))
            .unwrap_err();
        assert_eq!(drops.get(), 0);
        assert!(Rc::ptr_eq(&rejected.0, &drops));
        assert_eq!((q.len(), q.next, q.next_tick()), (1, 1, Some(9)));
        drop(rejected);
        q.next = u64::MAX;
        assert_eq!(q.reschedule(first, 1), Some(9));
        assert_eq!(q.cancel_scope(&scope), Some(1));
        assert_eq!(drops.get(), 2);
        let next_scope = q.create_scope().unwrap();
        let rejected = q
            .schedule_scoped(&next_scope, 0, Payload(drops.clone()))
            .unwrap_err();
        assert_eq!(drops.get(), 2);
        assert!(q.is_empty());
        assert_eq!(q.next, u64::MAX);
        drop(rejected);
        assert_eq!(drops.get(), 3);
    }

    #[test]
    fn exhausted_scope_generations_retire_without_wrapping_or_consuming_task_ids() {
        let mut q = TickScheduler::with_scope_capacity(2, 2);
        q.scopes[0].generation = u64::MAX - 1;
        let last = q.create_scope().unwrap();
        assert_eq!(last.key.generation, u64::MAX);
        q.schedule_scoped(&last, 0, 7).unwrap();
        assert_eq!(q.cancel_scope(&last), Some(1));
        let second = q.create_scope().unwrap();
        assert_eq!(second.key.slot, 1);
        assert_eq!(q.cancel_scope(&last), None);
        assert_eq!(q.schedule_scoped(&last, 0, 8), Err(8));
        assert_eq!(q.next, 1);
        q.scopes[1].generation = u64::MAX;
        q.scopes[1].active = false;
        assert!(q.create_scope().is_none());
        assert_eq!(q.schedule(0, 9).unwrap().0, 1);
    }

    #[test]
    fn cancellation_never_invokes_callable_payloads() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let mut q = TickScheduler::with_scope_capacity(3, 1);
        let scope = q.create_scope().unwrap();
        let make_job = || || calls.set(calls.get() + 1);
        assert!(q.schedule_scoped(&scope, 1, make_job()).is_ok());
        assert!(q.schedule_scoped(&scope, 2, make_job()).is_ok());
        assert!(q.schedule(3, make_job()).is_ok());
        assert_eq!(q.cancel_scope(&scope), Some(2));
        assert_eq!(calls.get(), 0);
        for job in q.drain_ready_iter(3, 3) {
            job();
        }
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn cancellation_callback_panic_discards_tail_without_touching_other_jobs() {
        use std::{cell::Cell, rc::Rc};
        #[derive(Debug)]
        struct Payload(u8, Rc<Cell<usize>>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.1.set(self.1.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut q = TickScheduler::with_scope_capacity(5, 1);
        let scope = q.create_scope().unwrap();
        for value in 0..4 {
            q.schedule_scoped(&scope, value, Payload(value as u8, drops.clone()))
                .unwrap();
        }
        let plain = q.schedule(0, Payload(9, drops.clone())).unwrap();
        let calls = Cell::new(0);
        let mut retained = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            q.cancel_scope_with(&scope, |_, payload| {
                calls.set(calls.get() + 1);
                if payload.0 == 0 {
                    retained = Some(payload);
                } else {
                    panic!("cancel callback failed");
                }
            });
        }));
        assert!(result.is_err());
        assert_eq!(calls.get(), 2);
        assert_eq!(drops.get(), 3);
        assert_eq!(q.len(), 1);
        assert_eq!(q.next_tick(), Some(0));
        assert!(!q.is_scope_active(&scope));
        assert_eq!(q.cancel_scope(&scope), None);
        let replacement = q.create_scope().unwrap();
        q.schedule_scoped(&replacement, 0, Payload(7, drops.clone()))
            .unwrap();
        assert_eq!(q.cancel_scope(&scope), None);
        assert_eq!(q.cancel_scope(&replacement), Some(1));
        drop(retained);
        assert_eq!(drops.get(), 5);
        drop(q.cancel(plain));
        assert_eq!(drops.get(), 6);
    }

    #[test]
    fn cancellation_destructor_panic_invalidates_scope_and_discards_remaining_members() {
        use std::{cell::Cell, rc::Rc};
        #[derive(Debug)]
        struct Payload(bool, Rc<Cell<usize>>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.1.set(self.1.get() + 1);
                assert!(!self.0, "payload drop failed");
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut q = TickScheduler::with_scope_capacity(4, 1);
        let scope = q.create_scope().unwrap();
        for fail in [true, false, false] {
            q.schedule_scoped(&scope, 1, Payload(fail, drops.clone()))
                .unwrap();
        }
        q.schedule(9, Payload(false, drops.clone())).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            q.cancel_scope(&scope);
        }));
        assert!(result.is_err());
        assert_eq!(drops.get(), 3);
        assert_eq!(q.len(), 1);
        assert_eq!(q.next_tick(), Some(9));
        assert!(!q.is_scope_active(&scope));
        assert!(q.create_scope().is_some());
        drop(q);
        assert_eq!(drops.get(), 4);
    }

    #[test]
    fn ties_cancellation_and_catchup_budget() {
        let mut q = TickScheduler::new(3);
        let id = q.schedule(9, "cancel").unwrap();
        q.schedule(4, "a").unwrap();
        q.schedule(4, "b").unwrap();
        assert_eq!(q.schedule(1, "overflow"), Err("overflow"));
        assert_eq!(q.cancel(id), Some("cancel"));
        assert_eq!(q.cancel(id), None);
        q.schedule(u64::MAX, "last").unwrap();
        assert!(q.drain_ready(3, 3).is_empty());
        assert_eq!(q.drain_ready(9, 1), ["a"]);
        assert!(q.drain_ready(9, 0).is_empty());
        assert_eq!(q.drain_ready(9, 1), ["b"]);
        assert_eq!(q.drain_ready(u64::MAX, 2), ["last"]);
        assert!(q.is_empty());
    }

    #[test]
    fn iterator_is_lazy_bounded_and_fused() {
        let mut q = TickScheduler::new(4);
        q.schedule(4, "first").unwrap();
        let second = q.schedule(4, "second").unwrap();
        let third = q.schedule(5, "third").unwrap();
        q.schedule(9, "future").unwrap();

        drop(q.drain_ready_iter(9, usize::MAX));
        assert_eq!(q.len(), 4);
        assert_eq!(q.drain_ready_iter(9, 0).next(), None);
        assert_eq!(q.drain_ready_iter(3, usize::MAX).next(), None);
        {
            let mut ready = q.drain_ready_iter(9, 1);
            assert_eq!(ready.next(), Some("first"));
            assert_eq!(ready.next(), None);
            assert_eq!(ready.next(), None);
        }
        {
            let mut ready = q.drain_ready_iter(9, 3);
            assert_eq!(ready.next(), Some("second"));
        }
        assert_eq!(q.cancel(second), None);
        assert_eq!(q.reschedule(third, 6), Some(5));
        {
            let mut ready = q.drain_ready_iter(6, usize::MAX);
            assert_eq!(ready.next(), Some("third"));
            assert_eq!(ready.next(), None);
            assert_eq!(ready.next(), None);
        }
        assert_eq!(q.next_tick(), Some(9));
        assert_eq!(q.drain_ready_iter(9, usize::MAX).next(), Some("future"));
        assert_eq!(q.drain_ready_iter(u64::MAX, usize::MAX).next(), None);
        assert!(q.is_empty());
    }

    #[test]
    fn buffer_appends_only_to_spare_capacity_and_reuses_storage() {
        let mut out = Vec::with_capacity(3);
        let capacity = out.capacity();
        let pointer = out.as_ptr();
        let mut q = TickScheduler::new(capacity + 2);
        for value in 0..capacity + 1 {
            q.schedule(5, value).unwrap();
        }
        q.schedule(8, capacity + 1).unwrap();

        out.push(usize::MAX);
        assert_eq!(q.drain_ready_into(4, usize::MAX, &mut out), 0);
        assert_eq!(q.drain_ready_into(8, 0, &mut out), 0);
        assert_eq!(q.drain_ready_into(5, usize::MAX, &mut out), capacity - 1);
        assert_eq!(out[0], usize::MAX);
        assert!(out[1..].iter().copied().eq(0..capacity - 1));
        assert_eq!(q.len(), 3);
        assert_eq!(q.drain_ready_into(8, usize::MAX, &mut out), 0);
        assert_eq!(q.next_tick(), Some(5));

        out.clear();
        assert_eq!(q.drain_ready_into(5, 1, &mut out), 1);
        assert_eq!(out, [capacity - 1]);
        assert_eq!(q.drain_ready_into(5, usize::MAX, &mut out), 1);
        assert_eq!(out, [capacity - 1, capacity]);
        assert_eq!(q.next_tick(), Some(8));
        out.clear();
        assert_eq!(q.drain_ready_into(8, usize::MAX, &mut out), 1);
        assert_eq!(out, [capacity + 1]);
        assert_eq!(q.drain_ready_into(u64::MAX, usize::MAX, &mut out), 0);
        assert_eq!(out.as_ptr(), pointer);
        assert_eq!(out.capacity(), capacity);
        assert!(q.is_empty());
    }

    #[test]
    fn zero_capacity_buffers_leave_work_queued_and_zst_buffers_work() {
        let mut q = TickScheduler::new(1);
        let id = q.schedule(0, 7).unwrap();
        let mut out = Vec::new();
        assert_eq!(q.drain_ready_into(u64::MAX, usize::MAX, &mut out), 0);
        assert!(out.is_empty());
        assert_eq!(out.capacity(), 0);
        assert_eq!(q.cancel(id), Some(7));

        let mut q = TickScheduler::new(2);
        q.schedule(0, ()).unwrap();
        q.schedule(0, ()).unwrap();
        let mut out = Vec::new();
        assert_eq!(q.drain_ready_into(0, usize::MAX, &mut out), 2);
        assert_eq!(out, [(), ()]);
        assert!(q.is_empty());
    }

    #[test]
    fn reschedule_preserves_identity_original_ties_and_capacity() {
        let mut q = TickScheduler::new(3);
        let first = q.schedule(9, "first").unwrap();
        let second = q.schedule(5, "second").unwrap();
        let third = q.schedule(5, "third").unwrap();
        assert_eq!(q.reschedule(first, 5), Some(9));
        assert_eq!(q.reschedule(second, 5), Some(5));
        assert_eq!(q.reschedule(third, 2), Some(5));
        assert_eq!(q.next_tick(), Some(2));
        assert_eq!(q.reschedule(third, 5), Some(2));
        assert_eq!(q.len(), 3);
        assert_eq!(q.next_tick(), Some(5));
        assert_eq!(q.schedule(0, "full"), Err("full"));
        assert_eq!(q.next, 3);
        assert_eq!(q.drain_ready(5, 3), ["first", "second", "third"]);
        assert_eq!(q.reschedule(first, 0), None);
        assert_eq!(q.cancel(second), None);
    }

    #[test]
    fn reschedule_absent_ids_is_atomic_and_cancellation_still_works() {
        let mut q = TickScheduler::new(2);
        let first = q.schedule(9, "first").unwrap();
        let second = q.schedule(5, "second").unwrap();
        assert_eq!(q.reschedule(TaskId(100), 0), None);
        assert_eq!(q.len(), 2);
        assert_eq!(q.next_tick(), Some(5));
        assert_eq!(q.reschedule(first, u64::MAX), Some(9));
        assert_eq!(q.cancel(first), Some("first"));
        assert_eq!(q.reschedule(first, 0), None);
        let third = q.schedule(5, "third").unwrap();
        assert_ne!(third, first);
        assert_eq!(q.cancel(first), None);
        assert_eq!(q.reschedule(second, 0), Some(5));
        assert_eq!(q.drain_ready(0, 1), ["second"]);
        assert_eq!(q.reschedule(second, 7), None);
        assert_eq!(q.drain_ready(5, 2), ["third"]);
        assert_eq!(q.reschedule(third, u64::MAX), None);
    }

    #[test]
    fn exhausted_ids_still_allow_reschedule_and_cancel() {
        let mut q = TickScheduler::new(2);
        let first = q.schedule(0, "first").unwrap();
        q.next = u64::MAX - 1;
        let last = q.schedule(u64::MAX, "last").unwrap();
        assert_eq!(q.next, u64::MAX);
        assert_eq!(q.reschedule(first, u64::MAX), Some(0));
        assert_eq!(q.reschedule(last, u64::MAX), Some(u64::MAX));
        assert_eq!(q.drain_ready(u64::MAX, 1), ["first"]);
        assert_eq!(q.schedule(0, "exhausted"), Err("exhausted"));
        assert_eq!(q.reschedule(last, 0), Some(u64::MAX));
        assert_eq!(q.cancel(last), Some("last"));
        assert_eq!(q.schedule(0, "still exhausted"), Err("still exhausted"));
        assert_eq!(q.reschedule(last, 0), None);

        let mut zero = TickScheduler::new(0);
        assert_eq!(zero.schedule(0, "rejected"), Err("rejected"));
        assert_eq!(zero.reschedule(TaskId(0), 1), None);
        assert_eq!(zero.next, 0);
        assert_eq!(zero.drain_ready_iter(u64::MAX, usize::MAX).next(), None);
    }

    #[test]
    fn unyielded_owned_tasks_survive_early_drop_and_consumer_panic() {
        use std::{cell::Cell, rc::Rc};

        #[derive(Debug)]
        struct Payload(Rc<Cell<usize>>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        let mut q = TickScheduler::new(3);
        let first = q.schedule(0, Payload(Rc::clone(&drops))).unwrap();
        let second = q.schedule(0, Payload(Rc::clone(&drops))).unwrap();
        let third = q.schedule(0, Payload(Rc::clone(&drops))).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut ready = q.drain_ready_iter(0, 3);
            let _payload = ready.next().unwrap();
            panic!("consumer failed");
        }));
        assert!(result.is_err());
        assert_eq!(drops.get(), 1);
        assert_eq!(q.len(), 2);
        assert_eq!(q.cancel(first).map(drop), None);
        assert_eq!(q.reschedule(second, 1), Some(0));
        assert_eq!(q.cancel(third).map(drop), Some(()));
        assert_eq!(drops.get(), 2);
        let mut out = Vec::with_capacity(1);
        assert_eq!(q.drain_ready_into(1, 1, &mut out), 1);
        assert_eq!(drops.get(), 2);
        assert!(q.is_empty());
        out.clear();
        assert_eq!(drops.get(), 3);
    }
}
