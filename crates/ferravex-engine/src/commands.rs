//! Deferred host/world mutations at an explicit synchronization boundary.
//!
//! The command count is bounded; closures own arbitrary host-controlled payloads.
//! Inspection reports queue state and activity without invoking commands. Accepted
//! means queued, not successfully applied. Capacity rejection returns ownership to
//! the caller and does not reorder retained commands.
//!
//! Apply and drain consume commands in insertion order. Apply removes each command
//! immediately before invoking it. Panics propagate: earlier changes and any changes
//! made by the panicking command remain, that command is consumed, and all later
//! commands stay queued. There is no rollback or automatic retry. A lazy drain only
//! removes commands actually yielded; dropping it leaves the rest queued.
//!
//! [`FallibleCommandBuffer`] adds typed domain errors with an explicit
//! [`CommandFailurePolicy`]. Its error callback receives the caller-supplied
//! command ID and owned error without an intermediate error collection.
//!
//! ```
//! use ferravex_engine::commands::CommandBuffer;
//!
//! let mut commands = CommandBuffer::new(2);
//! assert_eq!(commands.capacity(), 2);
//! assert!(commands.push(|world: &mut Vec<u32>| world.push(10)).is_ok());
//! assert!(commands.push(|world: &mut Vec<u32>| world.push(20)).is_ok());
//! assert!(commands.is_full());
//! assert_eq!(commands.remaining_capacity(), 0);
//! let rejected = commands.push(|world: &mut Vec<u32>| world.push(30)).err().unwrap();
//! assert_eq!(commands.metrics().accepted, 2);
//! assert_eq!(commands.metrics().rejected, 1);
//!
//! let mut world = Vec::new();
//! let outcome = commands.apply_report(&mut world, 1);
//! assert_eq!((outcome.applied, outcome.remaining), (1, 1));
//! assert!(commands.push(rejected).is_ok());
//! assert_eq!(commands.apply(&mut world, usize::MAX), 2);
//! assert_eq!(world, [10, 20, 30]);
//! assert!(commands.is_empty());
//! let interval = commands.reset_metrics();
//! assert_eq!((interval.started, interval.applied), (3, 3));
//! assert_eq!(interval.high_water_mark, 2);
//! assert_eq!(commands.metrics().accepted, 0);
//! ```
use std::collections::VecDeque;
use std::iter::FusedIterator;

/// An owned, opaque deferred mutation. Calling it consumes it exactly once.
///
/// Returned by [`CommandDrain`]; payloads need not implement `Clone` or `Debug`.
/// Commands invoked outside their buffer are not included in its apply metrics.
pub type Command<W> = Box<dyn FnOnce(&mut W)>;

/// Queue activity since construction or the last metrics reset.
///
/// Counters saturate independently at [`u64::MAX`]. Commands may have been accepted
/// in a previous interval, so activity counts need not balance after a reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandBufferMetrics {
    /// Pushes accepted into the queue, including commands not yet applied.
    pub accepted: u64,
    /// Pushes rejected at capacity, including every push at zero capacity.
    pub rejected: u64,
    /// Commands removed for invocation by the buffer, including those that panic.
    pub started: u64,
    /// Commands invoked by the buffer that returned normally.
    pub applied: u64,
    /// Commands handed to callers by a drain, regardless of their later use.
    pub drained: u64,
    /// Commands removed by [`CommandBuffer::clear`], without invoking them.
    pub discarded: u64,
    /// Largest queued count, including the count at the last metrics reset.
    pub high_water_mark: usize,
}

/// A completed, bounded apply operation. A panic propagates instead of returning
/// an outcome; [`CommandBuffer::metrics`] still records its partial progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub struct CommandApplyOutcome {
    /// Commands invoked by this operation that returned normally.
    pub applied: usize,
    /// Commands still queued after this operation.
    pub remaining: usize,
}

/// A snapshot of a lazy drain's progress, with no command execution implied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub struct CommandDrainOutcome {
    /// Commands yielded by this drain so far.
    pub drained: usize,
    /// All commands still queued, including any outside the drain's budget.
    pub remaining: usize,
}

/// A bounded FIFO queue of opaque deferred mutations.
///
/// Queue storage grows lazily and is retained when commands are removed. Bounds
/// apply to the number of commands, not their payload sizes or execution time.
pub struct CommandBuffer<W> {
    commands: VecDeque<Command<W>>,
    capacity: usize,
    metrics: CommandBufferMetrics,
}
impl<W> CommandBuffer<W> {
    /// Creates an empty queue. Zero capacity is valid and rejects every push.
    pub fn new(capacity: usize) -> Self {
        Self {
            commands: VecDeque::new(),
            capacity,
            metrics: CommandBufferMetrics::default(),
        }
    }
    /// Queues a command without executing it, or returns it unchanged if full.
    ///
    /// Rejection neither boxes nor drops the supplied closure. A returned closure
    /// may be retried after capacity becomes available; it then joins the tail.
    pub fn push<F: FnOnce(&mut W) + 'static>(&mut self, command: F) -> Result<(), F> {
        if self.is_full() {
            self.metrics.rejected = self.metrics.rejected.saturating_add(1);
            return Err(command);
        }
        self.commands.push_back(Box::new(command));
        self.metrics.accepted = self.metrics.accepted.saturating_add(1);
        self.metrics.high_water_mark = self.metrics.high_water_mark.max(self.commands.len());
        Ok(())
    }
    /// Applies at most `limit` commands in insertion order, returning the number
    /// that completed. A zero limit does nothing; later commands remain queued.
    ///
    /// Has the same panic and partial-application semantics as [`Self::apply_report`].
    pub fn apply(&mut self, world: &mut W, limit: usize) -> usize {
        self.apply_report(world, limit).applied
    }
    /// Applies at most `limit` oldest commands and reports completed and remaining
    /// work without allocating additional storage. A zero limit does nothing.
    ///
    /// # Panics
    ///
    /// A command panic propagates; this method returns no outcome in that case.
    /// The panicking command has already been removed and increments `started`,
    /// but not `applied`. All later commands remain queued in order, even within
    /// this call's budget. Earlier changes and the panicking command's changes
    /// remain in `world`. Catching a panic does not establish that `world` is valid
    /// for further processing; the host owns that decision.
    ///
    /// ```
    /// use ferravex_engine::commands::CommandBuffer;
    /// use std::panic::{catch_unwind, AssertUnwindSafe};
    ///
    /// let mut commands = CommandBuffer::new(3);
    /// assert!(commands.push(|world: &mut Vec<u32>| world.push(1)).is_ok());
    /// assert!(commands.push(|world: &mut Vec<u32>| {
    ///     world.push(2);
    ///     panic!("host command failed");
    /// }).is_ok());
    /// assert!(commands.push(|world: &mut Vec<u32>| world.push(3)).is_ok());
    /// let mut world = Vec::new();
    /// let result = catch_unwind(AssertUnwindSafe(|| commands.apply_report(&mut world, 3)));
    /// assert!(result.is_err());
    /// assert_eq!(world, [1, 2]);
    /// assert_eq!(commands.len(), 1);
    /// assert_eq!((commands.metrics().started, commands.metrics().applied), (2, 1));
    /// commands.clear(); // The host chooses to discard the unexecuted tail.
    /// assert_eq!(commands.metrics().discarded, 1);
    /// ```
    pub fn apply_report(&mut self, world: &mut W, limit: usize) -> CommandApplyOutcome {
        let count = limit.min(self.commands.len());
        for _ in 0..count {
            let command = self.commands.pop_front().unwrap();
            self.metrics.started = self.metrics.started.saturating_add(1);
            command(world);
            self.metrics.applied = self.metrics.applied.saturating_add(1);
        }
        CommandApplyOutcome {
            applied: count,
            remaining: self.commands.len(),
        }
    }
    /// Lazily transfers up to `limit` oldest commands without invoking them or
    /// allocating additional storage. A zero limit yields nothing.
    ///
    /// Each iterator step removes one command. Dropping the iterator leaves all
    /// unyielded commands queued in their original order, even within its budget.
    /// Use [`CommandDrain::outcome`] before dropping it to inspect progress.
    ///
    /// ```
    /// use ferravex_engine::commands::{Command, CommandBuffer, CommandDrainOutcome};
    ///
    /// let mut commands = CommandBuffer::new(3);
    /// for value in 1..=3 {
    ///     assert!(commands.push(move |world: &mut Vec<u32>| world.push(value)).is_ok());
    /// }
    /// let mut drain = commands.drain_up_to(2);
    /// assert_eq!(drain.len(), 2); // Available within the budget, not total queued.
    /// let first: Command<Vec<u32>> = drain.next().unwrap();
    /// assert_eq!(drain.outcome(), CommandDrainOutcome { drained: 1, remaining: 2 });
    /// drop(drain); // Both unyielded commands remain queued.
    /// let mut world = Vec::new();
    /// first(&mut world);
    /// assert_eq!(commands.metrics().drained, 1);
    /// assert_eq!(commands.metrics().applied, 0); // External execution is not tracked.
    /// assert_eq!(commands.apply(&mut world, usize::MAX), 2);
    /// assert_eq!(world, [1, 2, 3]);
    /// ```
    pub fn drain_up_to(&mut self, limit: usize) -> CommandDrain<'_, W> {
        let remaining = limit.min(self.commands.len());
        CommandDrain {
            buffer: self,
            remaining,
            drained: 0,
        }
    }
    /// Number of commands awaiting application or draining.
    pub fn len(&self) -> usize {
        self.commands.len()
    }
    /// Whether no commands are queued.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
    /// Maximum queued command count, independent of backing storage capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Number of additional commands that fit before pushes are rejected.
    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.commands.len()
    }
    /// Whether pushes will be rejected. True for a zero-capacity queue.
    pub fn is_full(&self) -> bool {
        self.commands.len() == self.capacity
    }
    /// Copies metrics without consuming, invoking or inspecting opaque closures.
    pub fn metrics(&self) -> CommandBufferMetrics {
        self.metrics
    }
    /// Returns the previous metrics and starts a new observation interval.
    /// Queued commands remain intact; the high-water mark starts at their count.
    pub fn reset_metrics(&mut self) -> CommandBufferMetrics {
        std::mem::replace(
            &mut self.metrics,
            CommandBufferMetrics {
                high_water_mark: self.commands.len(),
                ..CommandBufferMetrics::default()
            },
        )
    }
    /// Discards all commands without invoking them, retaining storage and metrics.
    ///
    /// Each command is removed and counted before dropping its payload. If a
    /// payload destructor panics, that panic propagates and later commands remain
    /// queued. Use [`Self::drain_up_to`] to bound the number of commands discarded.
    pub fn clear(&mut self) {
        while let Some(command) = self.commands.pop_front() {
            self.metrics.discarded = self.metrics.discarded.saturating_add(1);
            drop(command);
        }
    }
}

/// A lazy, bounded FIFO drain created by [`CommandBuffer::drain_up_to`].
///
/// Dropping it preserves unyielded commands. Its exact iterator length counts
/// commands available within its remaining budget; [`Self::outcome`] reports all
/// remaining queued work. It never invokes commands itself.
#[must_use = "drains are lazy; only commands actually yielded are removed"]
pub struct CommandDrain<'a, W> {
    buffer: &'a mut CommandBuffer<W>,
    remaining: usize,
    drained: usize,
}
impl<W> CommandDrain<'_, W> {
    /// Returns progress so far without advancing the drain or invoking commands.
    pub fn outcome(&self) -> CommandDrainOutcome {
        CommandDrainOutcome {
            drained: self.drained,
            remaining: self.buffer.commands.len(),
        }
    }
}
impl<W> Iterator for CommandDrain<'_, W> {
    type Item = Command<W>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let command = self.buffer.commands.pop_front()?;
        self.remaining -= 1;
        self.drained += 1;
        self.buffer.metrics.drained = self.buffer.metrics.drained.saturating_add(1);
        Some(command)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<W> ExactSizeIterator for CommandDrain<'_, W> {}
impl<W> FusedIterator for CommandDrain<'_, W> {}

/// How a fallible apply responds to a command returning `Err`.
///
/// Neither policy catches panics, rolls back mutations, or retries commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandFailurePolicy {
    /// Report the first failure and leave every later command queued in order.
    StopOnError,
    /// Report each failure and continue within the same invocation budget.
    Continue,
}

/// A consumed command's caller-supplied ID and original, owned domain error.
///
/// No `Clone`, `Debug`, or [`std::error::Error`] bound is required to use the queue.
#[derive(Debug, PartialEq, Eq)]
pub struct CommandFailure<E> {
    /// The ID passed to [`FallibleCommandBuffer::push`].
    pub id: u64,
    /// The command's error; mutations made before returning it remain in the world.
    pub error: E,
}

/// Fallible queue activity since construction or the last metrics reset.
///
/// Counters saturate independently at [`u64::MAX`]. Pending commands can span
/// observation intervals, so accepted and completed counts need not balance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FallibleCommandBufferMetrics {
    /// Pushes accepted into the queue, including commands not yet invoked.
    pub accepted: u64,
    /// Pushes rejected at capacity, including every push at zero capacity.
    pub rejected: u64,
    /// Commands removed for invocation, including those that panic.
    pub started: u64,
    /// Invocations that returned `Ok(())`.
    pub applied: u64,
    /// Invocations that returned `Err`, counted before calling the error callback.
    pub failed: u64,
    /// Commands removed by [`FallibleCommandBuffer::clear`] without invoking them.
    pub discarded: u64,
    /// Largest queued count, including the count at the last metrics reset.
    pub high_water_mark: usize,
}

/// Counts from one normally returning fallible apply.
///
/// `started == applied + failed`, and `started` never exceeds the supplied limit.
/// A panic returns no outcome; queue metrics still record partial progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub struct FallibleCommandApplyOutcome {
    /// Commands invoked, including those returning a domain error.
    pub started: usize,
    /// Commands returning `Ok(())`.
    pub applied: usize,
    /// Commands returning `Err`, each delivered to the error callback.
    pub failed: usize,
    /// All commands still queued, including work outside the invocation budget.
    pub remaining: usize,
}

type FallibleCommand<W, E> = Box<dyn FnOnce(&mut W) -> Result<(), E>>;

/// A bounded FIFO queue of fallible deferred mutations.
///
/// This opt-in sibling leaves [`CommandBuffer`] and its normal-return accounting
/// unchanged. IDs are caller-supplied correlation labels, not handles: all `u64`
/// values are valid, and duplicate IDs are allowed. Assign distinct IDs when the
/// consumer needs to distinguish individual requests. No deduplication occurs.
///
/// Storage grows lazily on accepted pushes and is retained after application or
/// [`clear`](Self::clear). The capacity bounds command count, not payload/error
/// sizes or execution time. Applying does not allocate queue/report storage;
/// closures, error callbacks and destructors remain caller-controlled.
///
/// Commands can partially mutate before returning an error. They are removed
/// before invocation and are never automatically retried, even on failure.
/// Dropping the queue drops pending payloads without invoking their commands.
///
/// # Deferred inventory requests
///
/// Flush at an explicit simulation boundary. A capacity rejection returns the
/// unboxed closure; only that never-executed request is safe to enqueue again
/// without reasoning about partial application.
///
/// ```
/// use ferravex_engine::commands::{
///     CommandFailure, CommandFailurePolicy, FallibleCommandBuffer,
/// };
///
/// let mut commands = FallibleCommandBuffer::new(2);
/// let spend = |stock: &mut u32| {
///     *stock = stock.checked_sub(3).ok_or("insufficient stock")?;
///     Ok(())
/// };
/// assert!(commands.push(10, spend).is_ok());
/// assert!(commands.push(11, spend).is_ok());
/// let refill = commands.push(12, |stock| { *stock += 5; Ok(()) }).err().unwrap();
/// assert!(commands.is_full());
/// assert_eq!((commands.capacity(), commands.remaining_capacity()), (2, 0));
/// let mut stock = 4;
/// let mut failure = None;
/// let report = commands.apply_report(&mut stock, 2, CommandFailurePolicy::StopOnError,
///     |error| failure = Some(error));
/// assert_eq!((report.started, report.applied, report.failed, report.remaining), (2, 1, 1, 0));
/// assert_eq!(failure, Some(CommandFailure { id: 11, error: "insufficient stock" }));
/// assert_eq!(stock, 1);
/// assert!(commands.is_empty());
/// assert!(commands.push(12, refill).is_ok());
/// let report = commands.apply_report(&mut stock, 1, CommandFailurePolicy::Continue,
///     |_| panic!("refill cannot fail"));
/// assert_eq!((report.applied, stock), (1, 6));
/// assert_eq!(commands.metrics().failed, 1);
/// assert_eq!(commands.reset_metrics().accepted, 3);
/// assert_eq!(commands.metrics().started, 0);
/// assert!(commands.push(13, spend).is_ok());
/// commands.clear(); // A restart discards stale requests without spending stock.
/// assert_eq!((commands.len(), commands.metrics().discarded, stock), (0, 1, 6));
/// ```
pub struct FallibleCommandBuffer<W, E> {
    commands: VecDeque<(u64, FallibleCommand<W, E>)>,
    capacity: usize,
    metrics: FallibleCommandBufferMetrics,
}

impl<W, E> FallibleCommandBuffer<W, E> {
    /// Creates an empty queue. Zero capacity is valid and rejects every push.
    pub fn new(capacity: usize) -> Self {
        Self {
            commands: VecDeque::new(),
            capacity,
            metrics: FallibleCommandBufferMetrics::default(),
        }
    }

    /// Queues a command with a caller-supplied ID, or returns it unchanged if full.
    ///
    /// Rejection does not box, invoke or drop the closure. A rejected closure can
    /// be pushed again when space is available; it then joins the tail. IDs are
    /// preserved verbatim, including duplicates, and never affect execution order.
    pub fn push<F: FnOnce(&mut W) -> Result<(), E> + 'static>(
        &mut self,
        id: u64,
        command: F,
    ) -> Result<(), F> {
        if self.is_full() {
            self.metrics.rejected = self.metrics.rejected.saturating_add(1);
            return Err(command);
        }
        self.commands.push_back((id, Box::new(command)));
        self.metrics.accepted = self.metrics.accepted.saturating_add(1);
        self.metrics.high_water_mark = self.metrics.high_water_mark.max(self.commands.len());
        Ok(())
    }

    /// Invokes at most `limit` oldest commands, reporting every error as it occurs.
    ///
    /// Both successful and failed invocations consume budget. A zero limit runs
    /// neither commands nor `on_error`. Errors move to `on_error` in FIFO order,
    /// even under [`CommandFailurePolicy::StopOnError`]; the queue retains no error
    /// collection. Unexecuted commands stay queued in their original order.
    /// Under the stop policy, `failed == 1` means this call stopped on an error,
    /// including when that error was the last command or used the last budget unit.
    /// A later explicit apply resumes with the tail, never the failed command.
    ///
    /// # Panics
    ///
    /// Command and error-callback panics propagate under either policy. The
    /// current command is consumed, mutations remain, and the untouched tail stays
    /// queued. `started` is incremented before invocation; `applied` or `failed`
    /// is incremented on normal return, before `on_error`. A command panic does
    /// not become an `E`. The host must assess world validity before resuming.
    ///
    /// # A prepared system's explicit flush
    ///
    /// Propagate the original command ID and error through a named flush. The
    /// schedule suppresses its downstream consumers when the flush returns `Err`.
    ///
    /// ```
    /// use ferravex_engine::commands::{
    ///     CommandFailure, CommandFailurePolicy, FallibleCommandBuffer,
    /// };
    /// use ferravex_engine::systems::{SystemControl, SystemSchedule};
    ///
    /// struct Simulation {
    ///     stock: u32,
    ///     commands: FallibleCommandBuffer<u32, &'static str>,
    /// }
    /// let mut schedule = SystemSchedule::<Simulation, CommandFailure<&'static str>>::default();
    /// schedule.register_flush("inventory-flush", |sim| {
    ///     let mut failure = None;
    ///     let report = sim.commands.apply_report(
    ///         &mut sim.stock, 8, CommandFailurePolicy::StopOnError,
    ///         |error| failure = Some(error));
    ///     if let Some(error) = failure { return Err(error); }
    ///     // A successful but incomplete flush also suppresses dependent consumers.
    ///     Ok(if report.remaining == 0 { SystemControl::Continue } else { SystemControl::Stop })
    /// }).unwrap();
    /// schedule.register("observe", |sim| {
    ///     assert_eq!(sim.stock, 1);
    ///     Ok(SystemControl::Continue)
    /// }).unwrap();
    /// schedule.before_checked("inventory-flush", "observe").unwrap();
    /// schedule.prepare().unwrap();
    /// let mut sim = Simulation { stock: 3, commands: FallibleCommandBuffer::new(1) };
    /// assert!(sim.commands.push(42, |stock| {
    ///     *stock = stock.checked_sub(2).ok_or("insufficient stock")?;
    ///     Ok(())
    /// }).is_ok());
    /// assert_eq!(schedule.run(&mut sim).unwrap().invoked, 2);
    /// ```
    pub fn apply_report(
        &mut self,
        world: &mut W,
        limit: usize,
        policy: CommandFailurePolicy,
        mut on_error: impl FnMut(CommandFailure<E>),
    ) -> FallibleCommandApplyOutcome {
        let count = limit.min(self.commands.len());
        let mut outcome = FallibleCommandApplyOutcome::default();
        for _ in 0..count {
            let (id, command) = self.commands.pop_front().unwrap();
            self.metrics.started = self.metrics.started.saturating_add(1);
            outcome.started += 1;
            match command(world) {
                Ok(()) => {
                    self.metrics.applied = self.metrics.applied.saturating_add(1);
                    outcome.applied += 1;
                }
                Err(error) => {
                    self.metrics.failed = self.metrics.failed.saturating_add(1);
                    outcome.failed += 1;
                    on_error(CommandFailure { id, error });
                    if policy == CommandFailurePolicy::StopOnError {
                        break;
                    }
                }
            }
        }
        outcome.remaining = self.commands.len();
        outcome
    }

    /// Number of pending commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether no commands are queued.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Maximum queued command count, independent of backing storage capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of additional commands that fit before pushes are rejected.
    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.commands.len()
    }

    /// Whether pushes will be rejected. True for a zero-capacity queue.
    pub fn is_full(&self) -> bool {
        self.commands.len() == self.capacity
    }

    /// Copies metrics without consuming commands or inspecting closure payloads.
    pub fn metrics(&self) -> FallibleCommandBufferMetrics {
        self.metrics
    }

    /// Returns prior metrics and starts a new observation interval.
    /// Pending commands and IDs remain intact; the high-water mark starts at len.
    pub fn reset_metrics(&mut self) -> FallibleCommandBufferMetrics {
        std::mem::replace(
            &mut self.metrics,
            FallibleCommandBufferMetrics {
                high_water_mark: self.commands.len(),
                ..FallibleCommandBufferMetrics::default()
            },
        )
    }

    /// Drops all pending commands without invoking them, retaining storage/metrics.
    ///
    /// Each command is removed and counted before dropping its payload. If a
    /// destructor panics, the panic propagates and the untouched tail stays queued.
    /// This discards stale work on restart; it does not reset the caller's world.
    pub fn clear(&mut self) {
        while let Some(command) = self.commands.pop_front() {
            self.metrics.discarded = self.metrics.discarded.saturating_add(1);
            drop(command);
        }
    }
}

#[cfg(test)]
mod fallible_tests {
    use super::*;

    #[test]
    fn metrics_saturate_independently_and_reset_preserves_queued_ids() {
        let mut commands = FallibleCommandBuffer::new(1);
        let saturated = FallibleCommandBufferMetrics {
            accepted: u64::MAX,
            rejected: u64::MAX,
            started: u64::MAX,
            applied: u64::MAX,
            failed: u64::MAX,
            discarded: u64::MAX,
            high_water_mark: 1,
        };
        commands.metrics = saturated;
        assert!(commands.push(7, |_: &mut ()| Ok(())).is_ok());
        assert!(commands.push(8, |_| Ok(())).is_err());
        assert_eq!(
            commands
                .apply_report(
                    &mut (),
                    1,
                    CommandFailurePolicy::Continue,
                    |_| unreachable!()
                )
                .applied,
            1
        );
        assert!(commands.push(9, |_| Err(9)).is_ok());
        assert_eq!(
            commands
                .apply_report(&mut (), 1, CommandFailurePolicy::StopOnError, |failure| {
                    assert_eq!(failure, CommandFailure { id: 9, error: 9 });
                })
                .failed,
            1
        );
        assert!(commands.push(10, |_| Ok(())).is_ok());
        commands.clear();
        assert!(commands.push(11, |_| Err(11)).is_ok());
        assert_eq!(commands.metrics(), saturated);
        assert_eq!(commands.reset_metrics(), saturated);
        assert_eq!(
            commands.metrics(),
            FallibleCommandBufferMetrics {
                high_water_mark: 1,
                ..FallibleCommandBufferMetrics::default()
            }
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands
                .apply_report(&mut (), 1, CommandFailurePolicy::Continue, |failure| {
                    assert_eq!(failure, CommandFailure { id: 11, error: 11 });
                })
                .failed,
            1
        );
    }

    #[test]
    fn apply_and_clear_retain_storage_for_reuse() {
        let mut commands = FallibleCommandBuffer::<(), ()>::new(4);
        for id in 0..4 {
            assert!(commands.push(id, |_| Ok(())).is_ok());
        }
        let storage_capacity = commands.commands.capacity();
        assert_eq!(
            commands
                .apply_report(
                    &mut (),
                    1,
                    CommandFailurePolicy::Continue,
                    |_| unreachable!()
                )
                .applied,
            1
        );
        assert_eq!(commands.commands.capacity(), storage_capacity);
        commands.clear();
        assert_eq!(commands.commands.capacity(), storage_capacity);
        for id in 0..4 {
            assert!(commands.push(id, |_| Err(())).is_ok());
        }
        assert_eq!(
            commands
                .apply_report(&mut (), usize::MAX, CommandFailurePolicy::Continue, |_| {})
                .failed,
            4
        );
        assert_eq!(commands.commands.capacity(), storage_capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn world_changes_only_at_sync_and_respects_budget() {
        let mut world = crate::ecs::World::new();
        let mut commands = CommandBuffer::new(2);
        assert!(
            commands
                .push(|w: &mut crate::ecs::World| {
                    w.spawn((11_u32,));
                })
                .is_ok()
        );
        assert!(
            commands
                .push(|w: &mut crate::ecs::World| {
                    w.spawn((22_u32,));
                })
                .is_ok()
        );
        assert!(
            commands
                .push(|_: &mut crate::ecs::World| panic!("rejected command ran"))
                .is_err()
        );
        assert_eq!(world.query::<&u32>().into_iter().count(), 0);
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert_eq!(
            world
                .query::<&u32>()
                .into_iter()
                .map(|v| *v)
                .collect::<Vec<_>>(),
            [11]
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(commands.apply(&mut world, 3), 1);
        assert_eq!(world.query::<&u32>().into_iter().count(), 2);
        assert!(commands.is_empty());
    }

    #[test]
    fn inspection_and_reset_preserve_pending_commands() {
        let mut commands = CommandBuffer::new(2);
        assert_eq!(commands.capacity(), 2);
        assert_eq!(commands.remaining_capacity(), 2);
        assert!(!commands.is_full());
        assert_eq!(commands.metrics(), CommandBufferMetrics::default());
        for value in 1..=2 {
            assert!(
                commands
                    .push(move |world: &mut Vec<u32>| world.push(value))
                    .is_ok()
            );
        }
        assert!(commands.is_full());
        assert_eq!(commands.remaining_capacity(), 0);
        assert!(commands.push(|_| panic!("rejected command ran")).is_err());
        let metrics = CommandBufferMetrics {
            accepted: 2,
            rejected: 1,
            high_water_mark: 2,
            ..CommandBufferMetrics::default()
        };
        assert_eq!(commands.metrics(), metrics);
        assert_eq!(commands.metrics(), metrics);
        assert_eq!(commands.reset_metrics(), metrics);
        assert_eq!(
            commands.metrics(),
            CommandBufferMetrics {
                high_water_mark: 2,
                ..CommandBufferMetrics::default()
            }
        );
        assert_eq!(commands.len(), 2);
        let mut world = Vec::new();
        assert_eq!(
            commands.apply_report(&mut world, 1),
            CommandApplyOutcome {
                applied: 1,
                remaining: 1,
            }
        );
        assert_eq!(world, [1]);
        assert_eq!(commands.remaining_capacity(), 1);
        assert_eq!(commands.metrics().accepted, 0);
        assert_eq!(commands.metrics().applied, 1);
        assert_eq!(commands.reset_metrics().high_water_mark, 2);
        assert_eq!(commands.metrics().high_water_mark, 1);
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert_eq!(world, [1, 2]);
    }

    #[test]
    fn zero_capacity_and_zero_or_oversized_budgets_are_explicit() {
        let mut commands = CommandBuffer::new(0);
        assert!(commands.is_empty());
        assert!(commands.is_full());
        assert_eq!((commands.capacity(), commands.remaining_capacity()), (0, 0));
        let command = commands
            .push(|world: &mut usize| *world += 1)
            .err()
            .unwrap();
        assert_eq!(commands.metrics().rejected, 1);
        assert_eq!(commands.metrics().accepted, 0);
        let mut world = 0;
        assert_eq!(
            commands.apply_report(&mut world, usize::MAX),
            CommandApplyOutcome::default()
        );
        let mut drain = commands.drain_up_to(usize::MAX);
        assert_eq!(drain.len(), 0);
        assert!(drain.next().is_none());
        assert_eq!(drain.outcome(), CommandDrainOutcome::default());
        drop(drain);
        command(&mut world);
        assert_eq!(world, 1);

        let mut commands = CommandBuffer::new(3);
        for digit in 1..=3 {
            assert!(
                commands
                    .push(move |world: &mut usize| *world = *world * 10 + digit)
                    .is_ok()
            );
        }
        let mut world = 0;
        assert_eq!(
            commands.apply_report(&mut world, 0),
            CommandApplyOutcome {
                applied: 0,
                remaining: 3
            }
        );
        assert_eq!(world, 0);
        assert_eq!(commands.metrics().started, 0);
        let mut drain = commands.drain_up_to(0);
        assert_eq!(
            drain.outcome(),
            CommandDrainOutcome {
                drained: 0,
                remaining: 3
            }
        );
        assert!(drain.next().is_none());
        assert!(drain.next().is_none());
        drop(drain);
        assert_eq!(
            commands.apply_report(&mut world, 2),
            CommandApplyOutcome {
                applied: 2,
                remaining: 1
            }
        );
        assert_eq!(world, 12);
        assert_eq!(
            commands.apply_report(&mut world, usize::MAX),
            CommandApplyOutcome {
                applied: 1,
                remaining: 0
            }
        );
        assert_eq!(world, 123);
        assert_eq!(
            (commands.metrics().started, commands.metrics().applied),
            (3, 3)
        );
        assert_eq!(commands.apply(&mut world, usize::MAX), 0);
    }

    struct DropProbe {
        drops: std::rc::Rc<std::cell::Cell<usize>>,
        panic: bool,
    }
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
            assert!(!self.panic, "payload drop failed");
        }
    }

    #[test]
    fn rejected_nonclone_payload_is_returned_and_can_be_retried_at_the_tail() {
        let drops = std::rc::Rc::new(std::cell::Cell::new(0));
        let payload = DropProbe {
            drops: drops.clone(),
            panic: false,
        };
        let mut commands = CommandBuffer::new(2);
        for value in 1..=2 {
            assert!(
                commands
                    .push(move |world: &mut Vec<u32>| world.push(value))
                    .is_ok()
            );
        }
        let rejected = commands
            .push(move |world: &mut Vec<u32>| {
                world.push(3);
                drop(payload);
            })
            .err()
            .unwrap();
        assert_eq!(drops.get(), 0);
        let mut world = Vec::new();
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert!(commands.push(rejected).is_ok());
        assert_eq!(drops.get(), 0);
        assert_eq!(commands.apply(&mut world, 2), 2);
        assert_eq!(drops.get(), 1);
        assert_eq!(world, [1, 2, 3]);
        assert_eq!(
            (commands.metrics().accepted, commands.metrics().rejected),
            (3, 1)
        );
    }

    #[test]
    fn drains_are_lazy_exact_bounded_and_preserve_unconsumed_commands() {
        let mut commands = CommandBuffer::new(4);
        for value in 1..=4 {
            assert!(
                commands
                    .push(move |world: &mut Vec<u32>| world.push(value))
                    .is_ok()
            );
        }
        drop(commands.drain_up_to(usize::MAX));
        assert_eq!(commands.len(), 4);
        assert_eq!(commands.metrics().drained, 0);
        let mut world = Vec::new();
        let mut drain = commands.drain_up_to(3);
        assert_eq!(drain.size_hint(), (3, Some(3)));
        drain.next().unwrap()(&mut world);
        assert_eq!(drain.len(), 2);
        assert_eq!(
            drain.outcome(),
            CommandDrainOutcome {
                drained: 1,
                remaining: 3
            }
        );
        assert_eq!(
            drain.outcome(),
            CommandDrainOutcome {
                drained: 1,
                remaining: 3
            }
        );
        drop(drain);
        assert_eq!(commands.remaining_capacity(), 1);
        assert!(commands.push(|world: &mut Vec<u32>| world.push(5)).is_ok());
        let mut drain = commands.drain_up_to(2);
        drain.next().unwrap()(&mut world);
        drain.next().unwrap()(&mut world);
        assert_eq!(drain.size_hint(), (0, Some(0)));
        assert!(drain.next().is_none());
        assert!(drain.next().is_none());
        assert_eq!(
            drain.outcome(),
            CommandDrainOutcome {
                drained: 2,
                remaining: 2
            }
        );
        drop(drain);
        assert_eq!(commands.metrics().started, 0);
        assert_eq!(commands.metrics().applied, 0);
        assert_eq!(commands.metrics().drained, 3);
        assert_eq!(commands.apply(&mut world, 2), 2);
        assert_eq!(world, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn drain_consumer_error_preserves_the_unvisited_tail() {
        let mut commands = CommandBuffer::new(3);
        for value in 1..=3 {
            assert!(
                commands
                    .push(move |world: &mut Vec<u32>| world.push(value))
                    .is_ok()
            );
        }
        let mut world = Vec::new();
        let mut drain = commands.drain_up_to(usize::MAX);
        let result = drain.try_for_each(|command| {
            command(&mut world);
            if world.len() == 2 {
                Err("consumer stopped")
            } else {
                Ok(())
            }
        });
        assert_eq!(result, Err("consumer stopped"));
        assert_eq!(
            drain.outcome(),
            CommandDrainOutcome {
                drained: 2,
                remaining: 1
            }
        );
        drop(drain);
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert_eq!(world, [1, 2, 3]);
        assert_eq!(
            (commands.metrics().drained, commands.metrics().applied),
            (2, 1)
        );
    }

    #[test]
    fn both_apply_apis_preserve_partial_mutations_and_order_after_a_panic() {
        for report in [false, true] {
            let mut commands = CommandBuffer::new(4);
            assert!(commands.push(|world: &mut Vec<u32>| world.push(1)).is_ok());
            assert!(
                commands
                    .push(|world: &mut Vec<u32>| {
                        world.push(2);
                        panic!("command failed");
                    })
                    .is_ok()
            );
            for value in 3..=4 {
                assert!(
                    commands
                        .push(move |world: &mut Vec<u32>| world.push(value))
                        .is_ok()
                );
            }
            let mut world = Vec::new();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if report {
                    commands.apply_report(&mut world, 3).applied
                } else {
                    commands.apply(&mut world, 3)
                }
            }));
            assert!(result.is_err());
            assert_eq!(world, [1, 2]);
            assert_eq!(commands.len(), 2);
            assert_eq!(
                (commands.metrics().started, commands.metrics().applied),
                (2, 1)
            );
            assert!(commands.push(|world: &mut Vec<u32>| world.push(5)).is_ok());
            assert_eq!(
                commands.apply_report(&mut world, usize::MAX),
                CommandApplyOutcome {
                    applied: 3,
                    remaining: 0
                }
            );
            assert_eq!(world, [1, 2, 3, 4, 5]);
            assert_eq!(
                (commands.metrics().started, commands.metrics().applied),
                (5, 4)
            );
        }
    }

    #[test]
    fn draining_a_panicking_command_only_consumes_the_yielded_prefix() {
        let mut commands = CommandBuffer::new(3);
        assert!(commands.push(|world: &mut Vec<u32>| world.push(1)).is_ok());
        assert!(
            commands
                .push(|world: &mut Vec<u32>| {
                    world.push(2);
                    panic!("external execution failed");
                })
                .is_ok()
        );
        assert!(commands.push(|world: &mut Vec<u32>| world.push(3)).is_ok());
        let mut world = Vec::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for command in commands.drain_up_to(3) {
                command(&mut world);
            }
        }));
        assert!(result.is_err());
        assert_eq!(world, [1, 2]);
        assert_eq!(commands.len(), 1);
        assert_eq!(
            (commands.metrics().drained, commands.metrics().started),
            (2, 0)
        );
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert_eq!(world, [1, 2, 3]);
    }

    #[test]
    fn bounded_discard_and_clear_release_payloads_without_invoking_commands() {
        let drops = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut commands = CommandBuffer::new(3);
        for _ in 0..3 {
            let payload = DropProbe {
                drops: drops.clone(),
                panic: false,
            };
            assert!(
                commands
                    .push(move |_: &mut ()| {
                        drop(payload);
                        panic!("discarded command ran");
                    })
                    .is_ok()
            );
        }
        let storage_capacity = commands.commands.capacity();
        let mut drain = commands.drain_up_to(2);
        for command in drain.by_ref() {
            drop(command);
        }
        assert_eq!(
            drain.outcome(),
            CommandDrainOutcome {
                drained: 2,
                remaining: 1
            }
        );
        drop(drain);
        assert_eq!(drops.get(), 2);
        commands.clear();
        assert_eq!(drops.get(), 3);
        assert!(commands.is_empty());
        assert_eq!(commands.commands.capacity(), storage_capacity);
        assert_eq!(
            commands.metrics(),
            CommandBufferMetrics {
                accepted: 3,
                drained: 2,
                discarded: 1,
                high_water_mark: 3,
                ..CommandBufferMetrics::default()
            }
        );
        assert!(commands.push(|_: &mut ()| {}).is_ok());
        assert_eq!(commands.apply(&mut (), 1), 1);
        assert_eq!(commands.commands.capacity(), storage_capacity);
    }

    #[test]
    fn clear_counts_a_panicking_drop_and_preserves_later_commands() {
        let drops = std::rc::Rc::new(std::cell::Cell::new(0));
        let payload = DropProbe {
            drops: drops.clone(),
            panic: true,
        };
        let mut commands = CommandBuffer::new(2);
        assert!(commands.push(move |_: &mut Vec<u32>| drop(payload)).is_ok());
        assert!(commands.push(|world: &mut Vec<u32>| world.push(2)).is_ok());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| commands.clear()));
        assert!(result.is_err());
        assert_eq!(drops.get(), 1);
        assert_eq!(commands.metrics().discarded, 1);
        assert_eq!(commands.metrics().started, 0);
        assert_eq!(commands.len(), 1);
        let mut world = Vec::new();
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert_eq!(world, [2]);
    }

    #[test]
    fn metrics_saturate_and_remain_inspectable() {
        let mut commands = CommandBuffer::new(1);
        let saturated = CommandBufferMetrics {
            accepted: u64::MAX,
            rejected: u64::MAX,
            started: u64::MAX,
            applied: u64::MAX,
            drained: u64::MAX,
            discarded: u64::MAX,
            high_water_mark: 1,
        };
        commands.metrics = saturated;
        assert!(commands.push(|_: &mut ()| {}).is_ok());
        assert!(commands.push(|_| {}).is_err());
        assert_eq!(commands.apply(&mut (), 1), 1);
        assert!(commands.push(|_| {}).is_ok());
        assert_eq!(commands.drain_up_to(1).count(), 1);
        assert!(commands.push(|_| {}).is_ok());
        commands.clear();
        assert_eq!(commands.metrics(), saturated);
        assert_eq!(commands.reset_metrics(), saturated);
        assert_eq!(commands.metrics(), CommandBufferMetrics::default());
    }
}
