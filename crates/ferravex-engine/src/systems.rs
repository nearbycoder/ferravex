//! Prepared, bounded sequential execution over the existing [`SystemOrder`].
//!
//! Register at most 256 named entries (systems and flush boundaries combined),
//! declare dependencies, then [`prepare`](SystemSchedule::prepare) once before
//! running. The cached sequence preserves [`SystemOrder::resolve`]'s earliest-ready
//! rule, not flattened layer order. Successful edits invalidate preparation;
//! rejected edits leave the previous prepared sequence usable.
//!
//! Each run starts at the beginning. Disabled entries bypass their predicates;
//! otherwise an optional predicate is evaluated immediately before invocation.
//! Skipping an entry does not skip its dependents: edges constrain ordering, not
//! whether prerequisites executed. Flush entries are ordinary named callbacks
//! marking explicit synchronization points, enabled unconditionally by default.
//! There is no implicit flush at the start, end, stop or error of a run.
//!
//! Stop/error suppresses the unvisited tail, including its predicates and flushes.
//! Mutations already made remain; no callback is rolled back or automatically
//! retried. Panics propagate with the same tail suppression and no returned report.
//! A later explicit run starts over, so the host must decide whether reuse is safe.
//! Clearing the schedule drops callbacks without invoking them. It does not reset
//! the caller's context, pending commands, session, or captured external state.
//!
//! Registration/preparation may allocate. Prepared execution performs no runner
//! allocations or name lookup; callback/predicate work and payload size are host
//! controlled. No clock, renderer, platform I/O, parallelism or borrow inference is
//! involved. Run once per **executed** tick; predicates must not freeze required
//! authoritative network/replay work just because a local menu is visible.
//!
//! # Fixed ticks and an explicit command flush
//!
//! This headless recipe uses the existing command queue. Only the named flush
//! publishes queued changes.
//!
//! ```
//! use ferravex_engine::{app::TickContext, commands::CommandBuffer, session::GameSession};
//! use ferravex_engine::systems::{SystemControl, SystemSchedule};
//!
//! struct Simulation {
//!     tick: TickContext,
//!     position: f32,
//!     commands: CommandBuffer<f32>,
//!     published: f32,
//! }
//! let mut schedule = SystemSchedule::<Simulation, &'static str>::default();
//! schedule.register("publish-check", |sim| {
//!     assert_eq!(sim.published, sim.position);
//!     Ok(SystemControl::Continue)
//! }).unwrap();
//! schedule.register("motion", |sim| {
//!     sim.position += 4.0 * sim.tick.dt;
//!     let position = sim.position;
//!     sim.commands.push(move |published| *published = position)
//!         .map_err(|_| "command capacity")?;
//!     Ok(SystemControl::Continue)
//! }).unwrap();
//! schedule.register_flush("flush", |sim| {
//!     sim.commands.apply(&mut sim.published, usize::MAX);
//!     Ok(SystemControl::Continue)
//! }).unwrap();
//! schedule.before_checked("motion", "flush").unwrap();
//! schedule.before_checked("flush", "publish-check").unwrap();
//! schedule.prepare().unwrap();
//! assert_eq!(schedule.prepared_names().unwrap().collect::<Vec<_>>(),
//!            ["motion", "flush", "publish-check"]);
//! let mut sim = Simulation {
//!     tick: TickContext { tick: 0, batch_index: 0, dt: 1.0 / 64.0 },
//!     position: 0.0, commands: CommandBuffer::new(1), published: 0.0,
//! };
//! let mut session = GameSession::new(64.0, 4, true);
//! assert_eq!(session.advance_batch(0.5 / 64.0).ticks().len(), 0);
//! let batch = session.advance_batch(2.0 / 64.0);
//! let mut executed = 0;
//! for tick in batch.ticks() {
//!     sim.tick = tick;
//!     executed += 1; // Includes a run that returns Stop or Err; not a commit count.
//!     let report = schedule.run(&mut sim).unwrap();
//!     assert_eq!((report.invoked, report.flushes), (3, 1));
//!     if report.control == SystemControl::Stop { break; }
//! }
//! assert_eq!(executed, 2);
//! assert_eq!(sim.tick.tick, 2);
//! assert_eq!(sim.published, 8.0 / 64.0);
//! ```

use crate::system_order::{OrderDiagnostic, OrderError, SystemOrder};
use std::fmt;

/// Whether to continue this schedule invocation. Stop is not a permanent latch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemControl {
    Continue,
    Stop,
}

/// The role of a named callback; both kinds obey the same ordering and predicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemEntryKind {
    System,
    Flush,
}

/// Counts for one run, including partial progress on callback error.
///
/// Counts describe invocations, not committed changes. A stopping/failing callback
/// is included in `invoked` (and `flushes` for a flush). `invoked + skipped` is the
/// visited prefix; `scheduled - invoked - skipped` is the suppressed tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct ScheduleReport {
    /// All prepared entries, including disabled entries and flushes.
    pub scheduled: usize,
    /// Callbacks invoked, including flushes and the callback returning Stop/Err.
    pub invoked: usize,
    /// Visited entries disabled explicitly or rejected by their predicate.
    pub skipped: usize,
    /// Flush callbacks invoked, a subset of `invoked`.
    pub flushes: usize,
    /// Stop on either an explicit Stop or a callback error.
    pub control: SystemControl,
}

/// Preparation rejected the entire schedule; no callback or predicate ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulePrepareError {
    /// The existing ordering graph could not resolve, with its named diagnostic.
    Order(OrderDiagnostic),
    /// First entry in sequential order with no callable, even if disabled.
    MissingCallable { name: String },
}

impl fmt::Display for SchedulePrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Order(error) => error.fmt(f),
            Self::MissingCallable { name } => {
                write!(
                    f,
                    "system {name:?} has no callable; use set_callback before prepare"
                )
            }
        }
    }
}

impl std::error::Error for SchedulePrepareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Order(error) => Some(error),
            Self::MissingCallable { .. } => None,
        }
    }
}

/// A run failure. Names borrow the schedule, avoiding allocation on failure.
#[derive(Debug, PartialEq, Eq)]
pub enum ScheduleRunError<'a, E> {
    /// No work ran. Prepare after construction, clear, or any successful edit.
    NotPrepared,
    /// The callback's original error and exact progress; no rollback or retry.
    Callback {
        name: &'a str,
        kind: SystemEntryKind,
        error: E,
        report: ScheduleReport,
    },
}

impl<E: fmt::Display> fmt::Display for ScheduleRunError<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPrepared => f.write_str("schedule is not prepared; call prepare before run"),
            Self::Callback {
                name, kind, error, ..
            } => {
                write!(f, "{kind:?} callback {name:?} failed: {error}")
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ScheduleRunError<'_, E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotPrepared => None,
            Self::Callback { error, .. } => Some(error),
        }
    }
}

type Callback<C, E> = Box<dyn FnMut(&mut C) -> Result<SystemControl, E>>;
type Predicate<C> = Box<dyn FnMut(&C) -> bool>;

struct Entry<C, E> {
    name: String,
    kind: SystemEntryKind,
    callback: Option<Callback<C, E>>,
    predicate: Option<Predicate<C>>,
    enabled: bool,
}

/// A reusable sequential runner with one [`SystemOrder`] as its dependency graph.
///
/// Names obey the graph's 1..=128 UTF-8 byte limit and exact case-sensitive
/// uniqueness. Its 256-entry bound includes unbound names and flushes. Closure
/// captures are owned (`'static`), need not be `Send`, and persist across runs.
/// The context is borrowed only during each call. See the module's runnable recipe.
pub struct SystemSchedule<C, E> {
    order: SystemOrder,
    entries: Vec<Entry<C, E>>,
    sequence: Vec<usize>,
    prepared: bool,
}

impl<C, E> Default for SystemSchedule<C, E> {
    fn default() -> Self {
        Self {
            order: SystemOrder::default(),
            entries: Vec::new(),
            sequence: Vec::new(),
            prepared: false,
        }
    }
}

impl<C, E> SystemSchedule<C, E> {
    /// Register an unbound system name. [`Self::prepare`] rejects it until bound.
    ///
    /// ```
    /// use ferravex_engine::systems::{SchedulePrepareError, SystemControl, SystemSchedule};
    /// let mut schedule = SystemSchedule::<u32, ()>::default();
    /// schedule.add("count").unwrap();
    /// assert_eq!(schedule.prepare(), Err(SchedulePrepareError::MissingCallable {
    ///     name: "count".into(),
    /// }));
    /// schedule.set_callback("count", |count| { *count += 1; Ok(SystemControl::Continue) }).unwrap();
    /// schedule.prepare().unwrap();
    /// let mut count = 0;
    /// assert_eq!(schedule.run(&mut count).unwrap().invoked, 1);
    /// assert_eq!(count, 1);
    /// ```
    pub fn add(&mut self, name: &str) -> Result<(), OrderError> {
        self.order.add(name)?;
        self.invalidate();
        self.entries.push(Entry {
            name: name.into(),
            kind: SystemEntryKind::System,
            callback: None,
            predicate: None,
            enabled: true,
        });
        Ok(())
    }

    /// Register a new enabled system and its callback. Rejected closures are dropped.
    /// See the module recipe; use [`Self::set_callback`] to replace a callable.
    pub fn register<F>(&mut self, name: &str, callback: F) -> Result<(), OrderError>
    where
        F: FnMut(&mut C) -> Result<SystemControl, E> + 'static,
    {
        self.register_entry(name, SystemEntryKind::System, callback)
    }

    /// Register an explicit, named flush boundary, counting toward the same limit.
    ///
    /// The callback owns what to flush and any work budget; the runner has no queue.
    /// Declare edges on both sides as in the module recipe. Merely naming a flush
    /// does not order all producers/consumers around it. Stop/error before the flush
    /// leaves pending work to the caller; a flush itself may also stop or fail.
    pub fn register_flush<F>(&mut self, name: &str, callback: F) -> Result<(), OrderError>
    where
        F: FnMut(&mut C) -> Result<SystemControl, E> + 'static,
    {
        self.register_entry(name, SystemEntryKind::Flush, callback)
    }

    fn register_entry<F>(
        &mut self,
        name: &str,
        kind: SystemEntryKind,
        callback: F,
    ) -> Result<(), OrderError>
    where
        F: FnMut(&mut C) -> Result<SystemControl, E> + 'static,
    {
        self.add(name)?;
        let entry = self.entries.last_mut().expect("just registered");
        entry.kind = kind;
        entry.callback = Some(Box::new(callback));
        Ok(())
    }

    /// Bind or replace a callable without changing its kind, predicate or enablement.
    /// The old/rejected closure is dropped. See [`Self::add`] for an example.
    pub fn set_callback<F>(&mut self, name: &str, callback: F) -> Result<(), OrderError>
    where
        F: FnMut(&mut C) -> Result<SystemControl, E> + 'static,
    {
        let index = self.index(name)?;
        self.invalidate();
        self.entries[index].callback = Some(Box::new(callback));
        Ok(())
    }

    /// Opt into a predicate, evaluated once per visited, enabled entry on current
    /// context. It may retain private state; it is not evaluated during preparation.
    ///
    /// ```
    /// use ferravex_engine::systems::{SystemControl, SystemSchedule};
    /// let mut schedule = SystemSchedule::<u32, ()>::default();
    /// schedule.register("count", |n| { *n += 1; Ok(SystemControl::Continue) }).unwrap();
    /// schedule.set_run_if("count", |n| *n < 1).unwrap();
    /// schedule.prepare().unwrap();
    /// let mut n = 0;
    /// assert_eq!(schedule.run(&mut n).unwrap().invoked, 1);
    /// assert_eq!(schedule.run(&mut n).unwrap().skipped, 1);
    /// schedule.clear_run_if("count").unwrap();
    /// schedule.set_enabled("count", false).unwrap();
    /// assert!(!schedule.is_prepared());
    /// schedule.prepare().unwrap();
    /// assert_eq!(schedule.run(&mut n).unwrap().skipped, 1);
    /// schedule.set_enabled("count", true).unwrap();
    /// schedule.prepare().unwrap();
    /// assert_eq!(schedule.run(&mut n).unwrap().invoked, 1);
    /// assert_eq!(n, 2);
    /// ```
    pub fn set_run_if<F>(&mut self, name: &str, predicate: F) -> Result<(), OrderError>
    where
        F: FnMut(&C) -> bool + 'static,
    {
        let index = self.index(name)?;
        self.invalidate();
        self.entries[index].predicate = Some(Box::new(predicate));
        Ok(())
    }

    /// Remove a predicate, leaving enablement unchanged. See [`Self::set_run_if`].
    pub fn clear_run_if(&mut self, name: &str) -> Result<(), OrderError> {
        let index = self.index(name)?;
        self.invalidate();
        self.entries[index].predicate = None;
        Ok(())
    }

    /// Set explicit enablement. A disabled flush is skipped too; no queue is applied.
    /// Disabling does not relax missing-callable validation. See [`Self::set_run_if`].
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), OrderError> {
        let index = self.index(name)?;
        self.invalidate();
        self.entries[index].enabled = enabled;
        Ok(())
    }

    /// Declare an edge, deferring cycle rejection to preparation as [`SystemOrder::before`]
    /// does. Unknown names are rejected without invalidating the prepared sequence.
    ///
    /// ```
    /// use ferravex_engine::systems::{SchedulePrepareError, SystemControl, SystemSchedule};
    /// let mut schedule = SystemSchedule::<(), ()>::default();
    /// schedule.register("loop", |_| Ok(SystemControl::Continue)).unwrap();
    /// schedule.before("loop", "loop").unwrap();
    /// assert!(matches!(schedule.prepare(), Err(SchedulePrepareError::Order(_))));
    /// assert!(!schedule.is_prepared());
    /// ```
    pub fn before(&mut self, first: &str, second: &str) -> Result<(), OrderError> {
        self.order.before(first, second)?;
        self.invalidate();
        Ok(())
    }

    /// Declare an edge transactionally through [`SystemOrder::before_checked`].
    /// A rejected cycle/missing endpoint preserves preparation. See the module recipe.
    pub fn before_checked(&mut self, first: &str, second: &str) -> Result<(), OrderDiagnostic> {
        self.order.before_checked(first, second)?;
        self.invalidate();
        Ok(())
    }

    /// Read the single underlying graph for inspection. No mutable graph escapes.
    pub fn order(&self) -> &SystemOrder {
        &self.order
    }

    /// Number of registered names, including flushes and unbound systems.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no registered names.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the current configuration has a complete, validated sequence.
    pub fn is_prepared(&self) -> bool {
        self.prepared
    }

    /// Inspect the exact cached sequence without allocation; `None` until prepared.
    /// All entries, even disabled ones, are included. See the module recipe.
    pub fn prepared_names(&self) -> Option<impl ExactSizeIterator<Item = &str> + '_> {
        self.prepared.then(|| {
            self.sequence
                .iter()
                .map(|&index| self.entries[index].name.as_str())
        })
    }

    /// Resolve once and require a callable for every name, including disabled ones.
    ///
    /// Uses [`SystemOrder::resolve_checked`], which delegates to the existing
    /// sequential [`SystemOrder::resolve`]. Graph errors take precedence over the
    /// first missing callable in that order. On failure no partial sequence is
    /// usable. Repeated preparation without edits is an allocation-free no-op.
    /// No callbacks or predicates run here. See [`Self::add`] and [`Self::before`].
    pub fn prepare(&mut self) -> Result<(), SchedulePrepareError> {
        if self.prepared {
            return Ok(());
        }
        self.sequence.clear();
        let names = self
            .order
            .resolve_checked()
            .map_err(SchedulePrepareError::Order)?;
        for name in names {
            let index = self.index(name).expect("all graph names have entries");
            if self.entries[index].callback.is_none() {
                self.sequence.clear();
                return Err(SchedulePrepareError::MissingCallable { name: name.into() });
            }
            self.sequence.push(index);
        }
        self.prepared = true;
        Ok(())
    }

    /// Invoke the prepared sequence with no runner allocation, stopping at Stop/Err.
    ///
    /// Never prepares implicitly. A run error borrows the failing name and retains
    /// the original `E` plus prefix counts. Successful runs return the same counts;
    /// `control` distinguishes completion from early Stop. No hidden cleanup or
    /// final flush runs. A subsequent explicit invocation starts at the beginning.
    ///
    /// ```
    /// use ferravex_engine::systems::{ScheduleRunError, SystemControl, SystemSchedule};
    /// let mut schedule = SystemSchedule::<u32, &'static str>::default();
    /// schedule.register("fail", |n| { *n += 1; Err("partial mutation") }).unwrap();
    /// schedule.register_flush("tail", |_| panic!("must not flush")).unwrap();
    /// let mut n = 0;
    /// assert_eq!(schedule.run(&mut n), Err(ScheduleRunError::NotPrepared));
    /// schedule.prepare().unwrap();
    /// let error = schedule.run(&mut n).unwrap_err();
    /// match error {
    ///     ScheduleRunError::Callback { name, error, report, .. } => {
    ///         assert_eq!((name, error), ("fail", "partial mutation"));
    ///         assert_eq!((report.scheduled, report.invoked, report.flushes), (2, 1, 0));
    ///         assert_eq!(report.control, SystemControl::Stop);
    ///     }
    ///     ScheduleRunError::NotPrepared => unreachable!(),
    /// }
    /// assert_eq!(n, 1);
    /// ```
    pub fn run(&mut self, context: &mut C) -> Result<ScheduleReport, ScheduleRunError<'_, E>> {
        if !self.prepared {
            return Err(ScheduleRunError::NotPrepared);
        }
        let mut report = ScheduleReport {
            scheduled: self.sequence.len(),
            invoked: 0,
            skipped: 0,
            flushes: 0,
            control: SystemControl::Continue,
        };
        for &index in &self.sequence {
            let entry = &mut self.entries[index];
            if !entry.enabled || entry.predicate.as_mut().is_some_and(|test| !test(context)) {
                report.skipped += 1;
                continue;
            }
            report.invoked += 1;
            report.flushes += usize::from(entry.kind == SystemEntryKind::Flush);
            let result = entry.callback.as_mut().expect("prepared callable")(context);
            match result {
                Ok(SystemControl::Continue) => {}
                Ok(SystemControl::Stop) => {
                    report.control = SystemControl::Stop;
                    return Ok(report);
                }
                Err(error) => {
                    report.control = SystemControl::Stop;
                    return Err(ScheduleRunError::Callback {
                        name: &self.entries[index].name,
                        kind: self.entries[index].kind,
                        error,
                        report,
                    });
                }
            }
        }
        Ok(report)
    }

    /// Reset to an empty, unprepared schedule; drop closures without calling them.
    /// Caller-owned context/queues are unaffected. Configuration storage is released.
    /// The empty state is installed before dropping captured values (which may panic).
    ///
    /// ```
    /// use ferravex_engine::systems::{SystemControl, SystemSchedule};
    /// let mut schedule = SystemSchedule::<(), ()>::default();
    /// schedule.register("old", |_| Ok(SystemControl::Stop)).unwrap();
    /// schedule.prepare().unwrap();
    /// schedule.clear();
    /// assert!(schedule.is_empty());
    /// assert_eq!(schedule.len(), 0);
    /// assert!(schedule.order().resolve().unwrap().is_empty());
    /// assert!(schedule.prepared_names().is_none());
    /// schedule.prepare().unwrap();
    /// assert_eq!(schedule.run(&mut ()).unwrap().control, SystemControl::Continue);
    /// ```
    pub fn clear(&mut self) {
        drop(std::mem::take(self));
    }

    fn index(&self, name: &str) -> Result<usize, OrderError> {
        self.entries
            .iter()
            .position(|entry| entry.name == name)
            .ok_or(OrderError::UnknownSystem)
    }

    fn invalidate(&mut self) {
        self.prepared = false;
        self.sequence.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandBuffer;
    use std::{cell::Cell, panic::AssertUnwindSafe, rc::Rc};

    fn keep_going<C>(_: &mut C) -> Result<SystemControl, &'static str> {
        Ok(SystemControl::Continue)
    }

    #[test]
    fn runs_sequential_resolution_not_flattened_layers() {
        let names = ["render", "physics", "input", "audio", "idle", "log"];
        let mut schedule = SystemSchedule::<Vec<&str>, &'static str>::default();
        for name in names {
            schedule
                .register(name, move |calls| {
                    calls.push(name);
                    Ok(SystemControl::Continue)
                })
                .unwrap();
        }
        for (a, b) in [
            ("input", "physics"),
            ("physics", "render"),
            ("audio", "log"),
        ] {
            schedule.before_checked(a, b).unwrap();
        }
        schedule.before_checked("input", "physics").unwrap();
        let expected = ["input", "physics", "render", "audio", "idle", "log"];
        assert_eq!(schedule.order().resolve().unwrap(), expected);
        assert_ne!(
            schedule.order().resolve_layers().unwrap().concat(),
            expected
        );
        schedule.prepare().unwrap();
        assert_eq!(
            schedule.prepared_names().unwrap().collect::<Vec<_>>(),
            expected
        );
        for _ in 0..3 {
            let mut calls = Vec::new();
            assert_eq!(
                schedule.run(&mut calls).unwrap(),
                ScheduleReport {
                    scheduled: 6,
                    invoked: 6,
                    skipped: 0,
                    flushes: 0,
                    control: SystemControl::Continue,
                }
            );
            assert_eq!(calls, expected);
        }
    }

    #[test]
    fn missing_even_disabled_callables_reject_all_work_in_sequential_order() {
        let calls = Rc::new(Cell::new(0));
        let mut schedule = SystemSchedule::<(), &'static str>::default();
        schedule.add("late").unwrap();
        let observed = calls.clone();
        schedule
            .register("ready", move |_| {
                observed.set(observed.get() + 1);
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.add("early").unwrap();
        schedule.set_enabled("early", false).unwrap();
        schedule.before("early", "late").unwrap();
        schedule
            .set_run_if("ready", |_| panic!("prepare must not invoke predicates"))
            .unwrap();
        assert_eq!(
            schedule.prepare(),
            Err(SchedulePrepareError::MissingCallable {
                name: "early".into()
            })
        );
        assert!(schedule.prepared_names().is_none());
        assert_eq!(schedule.run(&mut ()), Err(ScheduleRunError::NotPrepared));
        assert_eq!(calls.get(), 0);
        schedule.set_callback("early", keep_going).unwrap();
        assert_eq!(
            schedule.prepare(),
            Err(SchedulePrepareError::MissingCallable {
                name: "late".into()
            })
        );
        schedule.set_callback("late", keep_going).unwrap();
        schedule.clear_run_if("ready").unwrap();
        schedule.prepare().unwrap();
        let report = schedule.run(&mut ()).unwrap();
        assert_eq!((report.invoked, report.skipped), (2, 1));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn cycles_take_precedence_and_clear_recovers() {
        let mut schedule = SystemSchedule::<(), &'static str>::default();
        schedule
            .register("ready", |_| panic!("no partial execution"))
            .unwrap();
        schedule.add("unbound").unwrap();
        schedule.before("unbound", "unbound").unwrap();
        assert_eq!(
            schedule.prepare(),
            Err(SchedulePrepareError::Order(OrderDiagnostic::Cycle {
                path: vec!["unbound".into(), "unbound".into()],
            }))
        );
        assert_eq!(schedule.run(&mut ()), Err(ScheduleRunError::NotPrepared));
        schedule.clear();
        assert!(!schedule.is_prepared());
        assert!(schedule.is_empty());
        schedule.prepare().unwrap();
        assert_eq!(schedule.prepared_names().unwrap().len(), 0);
        assert_eq!(
            schedule.run(&mut ()).unwrap(),
            ScheduleReport {
                scheduled: 0,
                invoked: 0,
                skipped: 0,
                flushes: 0,
                control: SystemControl::Continue,
            }
        );
        schedule.register("unbound", keep_going).unwrap();
        schedule.prepare().unwrap();
        assert_eq!(schedule.run(&mut ()).unwrap().invoked, 1);
    }

    #[test]
    fn rejected_edits_preserve_preparation_and_successful_edits_require_it_again() {
        let mut schedule = SystemSchedule::<Vec<u32>, &'static str>::default();
        schedule
            .register("a", |v| {
                v.push(1);
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule
            .register_flush("b", |v| {
                v.push(2);
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.before_checked("a", "b").unwrap();
        schedule.prepare().unwrap();
        assert_eq!(schedule.add(""), Err(OrderError::InvalidName));
        assert_eq!(
            schedule.register("a", keep_going),
            Err(OrderError::Duplicate)
        );
        assert_eq!(
            schedule.set_callback("missing", keep_going),
            Err(OrderError::UnknownSystem)
        );
        assert_eq!(
            schedule.set_run_if("missing", |_| true),
            Err(OrderError::UnknownSystem)
        );
        assert_eq!(
            schedule.clear_run_if("missing"),
            Err(OrderError::UnknownSystem)
        );
        assert_eq!(
            schedule.set_enabled("missing", false),
            Err(OrderError::UnknownSystem)
        );
        assert_eq!(
            schedule.before("missing", "a"),
            Err(OrderError::UnknownSystem)
        );
        assert!(matches!(
            schedule.before_checked("missing", "a"),
            Err(OrderDiagnostic::UnknownSystems { .. })
        ));
        assert!(matches!(
            schedule.before_checked("b", "a"),
            Err(OrderDiagnostic::Cycle { .. })
        ));
        assert!(schedule.is_prepared());
        let mut values = Vec::new();
        assert_eq!(schedule.run(&mut values).unwrap().flushes, 1);
        assert_eq!(values, [1, 2]);

        for edit in 0..8 {
            match edit {
                0 => schedule
                    .set_callback("b", |v| {
                        v.push(3);
                        Ok(SystemControl::Continue)
                    })
                    .unwrap(),
                1 => schedule.set_run_if("b", |_| false).unwrap(),
                2 => schedule.clear_run_if("b").unwrap(),
                3 => schedule.set_enabled("b", false).unwrap(),
                4 => schedule.set_enabled("b", true).unwrap(),
                5 => schedule.before_checked("a", "b").unwrap(),
                6 => schedule.before("a", "b").unwrap(),
                _ => schedule.register("c", keep_going).unwrap(),
            }
            assert!(!schedule.is_prepared());
            assert!(schedule.prepared_names().is_none());
            assert_eq!(
                schedule.run(&mut values),
                Err(ScheduleRunError::NotPrepared)
            );
            schedule.prepare().unwrap();
            values.clear();
            let report = schedule.run(&mut values).unwrap();
            if edit == 1 || edit == 3 {
                assert_eq!(values, [1]);
                assert_eq!((report.skipped, report.flushes), (1, 0));
            } else {
                assert_eq!(values, [1, 3]);
                assert_eq!(report.flushes, 1); // Callback replacement kept its kind.
            }
        }
        // A new constraint must actually change the cached order after preparation.
        schedule.before_checked("c", "a").unwrap();
        schedule.prepare().unwrap();
        assert_eq!(
            schedule.prepared_names().unwrap().collect::<Vec<_>>(),
            ["c", "a", "b"]
        );
    }

    #[test]
    fn predicates_observe_current_context_and_skips_do_not_disable_dependents() {
        let mut schedule = SystemSchedule::<u32, &'static str>::default();
        schedule
            .register("disabled", |_| panic!("disabled"))
            .unwrap();
        schedule
            .set_run_if("disabled", |_| panic!("disabled predicate"))
            .unwrap();
        schedule.set_enabled("disabled", false).unwrap();
        schedule
            .register("increment", |n| {
                *n += 1;
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.before("disabled", "increment").unwrap();
        schedule
            .register("conditional", |n| {
                *n += 10;
                Ok(SystemControl::Continue)
            })
            .unwrap();
        let checked = Rc::new(Cell::new(0));
        let count = checked.clone();
        schedule
            .set_run_if("conditional", move |n| {
                count.set(count.get() + 1);
                *n == 1
            })
            .unwrap();
        schedule.before("increment", "conditional").unwrap();
        schedule.prepare().unwrap();
        assert_eq!(checked.get(), 0);
        let mut n = 0;
        assert_eq!(schedule.run(&mut n).unwrap().skipped, 1);
        assert_eq!(n, 11);
        assert_eq!(schedule.run(&mut n).unwrap().skipped, 2);
        assert_eq!(n, 12);
        assert_eq!(checked.get(), 2);
    }

    #[test]
    fn stop_and_error_at_every_position_suppress_all_tail_work_and_allow_explicit_rerun() {
        for fail in [false, true] {
            for boundary in 0..3 {
                for kind in [SystemEntryKind::System, SystemEntryKind::Flush] {
                    let mut schedule = SystemSchedule::<Vec<usize>, &'static str>::default();
                    let predicates = Rc::new(Cell::new(0));
                    for index in 0..3 {
                        let callback = move |calls: &mut Vec<usize>| {
                            calls.push(index);
                            if index == boundary {
                                if fail {
                                    Err("original error")
                                } else {
                                    Ok(SystemControl::Stop)
                                }
                            } else {
                                Ok(SystemControl::Continue)
                            }
                        };
                        let name = index.to_string();
                        if kind == SystemEntryKind::Flush {
                            schedule.register_flush(&name, callback).unwrap();
                        } else {
                            schedule.register(&name, callback).unwrap();
                        }
                        let predicates = predicates.clone();
                        schedule
                            .set_run_if(&name, move |_| {
                                predicates.set(predicates.get() + 1);
                                true
                            })
                            .unwrap();
                    }
                    schedule.prepare().unwrap();
                    for run in 1..=2 {
                        let mut calls = Vec::new();
                        let result = schedule.run(&mut calls);
                        let report = if fail {
                            let ScheduleRunError::Callback {
                                name,
                                kind: actual_kind,
                                error,
                                report,
                            } = result.unwrap_err()
                            else {
                                panic!("prepared schedule rejected");
                            };
                            assert_eq!(name, boundary.to_string());
                            assert_eq!(actual_kind, kind);
                            assert_eq!(error, "original error");
                            report
                        } else {
                            result.unwrap()
                        };
                        assert_eq!(calls, (0..=boundary).collect::<Vec<_>>());
                        assert_eq!(predicates.get(), run * (boundary + 1));
                        assert_eq!(
                            report,
                            ScheduleReport {
                                scheduled: 3,
                                invoked: boundary + 1,
                                skipped: 0,
                                flushes: if kind == SystemEntryKind::Flush {
                                    boundary + 1
                                } else {
                                    0
                                },
                                control: SystemControl::Stop,
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn explicit_flush_controls_visibility_budget_and_pending_tail_after_failure() {
        struct Context {
            world: Vec<u32>,
            queue: CommandBuffer<Vec<u32>>,
            fail: bool,
        }
        let mut schedule = SystemSchedule::<Context, &'static str>::default();
        schedule
            .register("produce", |c| {
                assert!(c.queue.push(|world| world.push(1)).is_ok());
                assert!(c.queue.push(|world| world.push(2)).is_ok());
                assert!(c.world.is_empty());
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule
            .register_flush("one-command", |c| {
                assert_eq!(c.queue.apply(&mut c.world, 1), 1);
                if c.fail {
                    Err("after partial flush")
                } else {
                    Ok(SystemControl::Continue)
                }
            })
            .unwrap();
        schedule
            .register("observe", |c| {
                assert_eq!(c.world, [1]);
                assert_eq!(c.queue.len(), 1);
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.before("produce", "one-command").unwrap();
        schedule.before("one-command", "observe").unwrap();
        schedule.prepare().unwrap();
        for fail in [false, true] {
            let mut c = Context {
                world: Vec::new(),
                queue: CommandBuffer::new(2),
                fail,
            };
            let result = schedule.run(&mut c);
            if fail {
                let ScheduleRunError::Callback {
                    name, kind, report, ..
                } = result.unwrap_err()
                else {
                    panic!()
                };
                assert_eq!((name, kind), ("one-command", SystemEntryKind::Flush));
                assert_eq!((report.invoked, report.flushes), (2, 1));
            } else {
                assert_eq!(result.unwrap().invoked, 3);
            }
            assert_eq!(c.world, [1]);
            assert_eq!(c.queue.len(), 1); // No implicit final/error flush.
            assert_eq!(c.queue.apply(&mut c.world, 1), 1);
            assert_eq!(c.world, [1, 2]);
        }
        schedule
            .set_callback("produce", |c| {
                assert!(c.queue.push(|w| w.push(9)).is_ok());
                Ok(SystemControl::Stop)
            })
            .unwrap();
        schedule.prepare().unwrap();
        let mut c = Context {
            world: Vec::new(),
            queue: CommandBuffer::new(2),
            fail: false,
        };
        assert_eq!(schedule.run(&mut c).unwrap().flushes, 0);
        schedule.clear();
        assert!(c.world.is_empty());
        assert_eq!(c.queue.len(), 1); // Clearing callbacks does not clear host commands.
    }

    #[test]
    fn capacity_names_and_rejected_capture_drop_are_bounded_and_atomic() {
        let mut schedule = SystemSchedule::<(), &'static str>::default();
        let unicode = "é".repeat(64);
        assert_eq!(schedule.add(&"é".repeat(65)), Err(OrderError::InvalidName));
        schedule.add(&unicode).unwrap();
        schedule.set_callback(&unicode, keep_going).unwrap();
        for index in 1..256 {
            if index % 2 == 0 {
                schedule.register(&index.to_string(), keep_going).unwrap();
            } else {
                schedule
                    .register_flush(&index.to_string(), keep_going)
                    .unwrap();
            }
        }
        schedule.prepare().unwrap();
        let captured = Rc::new(());
        let rejected = captured.clone();
        assert_eq!(
            schedule.register("excess", move |_| {
                let _ = &rejected;
                Ok(SystemControl::Continue)
            }),
            Err(OrderError::Capacity)
        );
        assert_eq!(Rc::strong_count(&captured), 1);
        assert_eq!(schedule.add(""), Err(OrderError::InvalidName));
        assert_eq!(schedule.add(&unicode), Err(OrderError::Duplicate));
        assert_eq!(schedule.len(), 256);
        assert!(schedule.is_prepared());
        assert_eq!(
            schedule.run(&mut ()).unwrap(),
            ScheduleReport {
                scheduled: 256,
                invoked: 256,
                skipped: 0,
                flushes: 128,
                control: SystemControl::Continue,
            }
        );
        schedule.clear();
        schedule.register("excess", keep_going).unwrap();
        schedule.prepare().unwrap();
        assert_eq!(schedule.run(&mut ()).unwrap().invoked, 1);
    }

    #[test]
    fn callback_and_predicate_panics_suppress_tail_without_implicit_flush() {
        for in_predicate in [false, true] {
            let mut schedule = SystemSchedule::<Vec<u32>, &'static str>::default();
            schedule
                .register("first", |v| {
                    v.push(1);
                    Ok(SystemControl::Continue)
                })
                .unwrap();
            schedule
                .register("panic", |v| {
                    v.push(2);
                    panic!("callback")
                })
                .unwrap();
            if in_predicate {
                schedule
                    .set_run_if("panic", |_| panic!("predicate"))
                    .unwrap();
            }
            schedule
                .register_flush("tail", |v| {
                    v.push(3);
                    Ok(SystemControl::Continue)
                })
                .unwrap();
            schedule.prepare().unwrap();
            let mut v = Vec::new();
            assert!(
                std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let _ = schedule.run(&mut v);
                }))
                .is_err()
            );
            assert_eq!(v, if in_predicate { vec![1] } else { vec![1, 2] });
            schedule.clear();
            assert!(schedule.is_empty());
        }
    }

    #[test]
    fn clear_installs_empty_state_before_dropping_captures_and_preserves_context() {
        struct PanicOnDrop;
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("capture drop");
            }
        }
        let mut schedule = SystemSchedule::<u32, &'static str>::default();
        let capture = PanicOnDrop;
        schedule
            .register("old", move |_| {
                let _ = &capture;
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.prepare().unwrap();
        assert!(std::panic::catch_unwind(AssertUnwindSafe(|| schedule.clear())).is_err());
        assert!(schedule.is_empty());
        assert!(schedule.order().resolve().unwrap().is_empty());
        assert!(!schedule.is_prepared());
        schedule
            .register("new", |v| {
                *v += 1;
                Ok(SystemControl::Continue)
            })
            .unwrap();
        schedule.prepare().unwrap();
        let mut value = 5;
        assert_eq!(schedule.run(&mut value).unwrap().invoked, 1);
        assert_eq!(value, 6);
    }

    #[test]
    fn borrowed_context_and_stateful_callbacks_do_not_require_send_or_static_context() {
        struct Context<'a>(&'a mut u32);
        let mut value = 0;
        {
            let mut schedule = SystemSchedule::<Context<'_>, &'static str>::default();
            let mut calls = 0;
            schedule
                .register("count", move |c| {
                    calls += 1;
                    *c.0 += calls;
                    Ok(SystemControl::Continue)
                })
                .unwrap();
            schedule.prepare().unwrap();
            let mut context = Context(&mut value);
            assert_eq!(schedule.run(&mut context).unwrap().invoked, 1);
            assert_eq!(schedule.run(&mut context).unwrap().invoked, 1);
        }
        assert_eq!(value, 3);
    }

    #[test]
    fn app_consumer_counts_only_executed_tick_contexts_and_suppresses_batch_tail() {
        use crate::{
            app::{App, AppControl, Game, TickContext},
            session::GameSession,
        };
        struct Context {
            ticks: Vec<TickContext>,
            current: Option<TickContext>,
            outcome: Result<SystemControl, &'static str>,
        }
        struct Host {
            schedule: SystemSchedule<Context, &'static str>,
            context: Context,
        }
        impl Game for Host {
            type Error = &'static str;
            fn fixed_update(&mut self, _: f32) -> Result<AppControl, Self::Error> {
                panic!("context hook owns dispatch")
            }
            fn fixed_update_context(
                &mut self,
                tick: TickContext,
            ) -> Result<AppControl, Self::Error> {
                self.context.current = Some(tick);
                match self.schedule.run(&mut self.context) {
                    Ok(report) => Ok(match report.control {
                        SystemControl::Continue => AppControl::Continue,
                        SystemControl::Stop => AppControl::Stop,
                    }),
                    Err(ScheduleRunError::Callback { error, .. }) => Err(error),
                    Err(ScheduleRunError::NotPrepared) => Err("configuration must be prepared"),
                }
            }
        }
        for outcome in [
            Ok(SystemControl::Continue),
            Ok(SystemControl::Stop),
            Err("tick error"),
        ] {
            let mut schedule = SystemSchedule::default();
            schedule
                .register("tick", |c: &mut Context| {
                    c.ticks.push(c.current.unwrap());
                    c.outcome
                })
                .unwrap();
            schedule.prepare().unwrap();
            let context = Context {
                ticks: Vec::new(),
                current: None,
                outcome,
            };
            let mut app = App::new(Host { schedule, context }, GameSession::new(64.0, 4, true));
            assert_eq!(app.frame_report(0.5 / 64.0).unwrap().executed_updates, 0);
            assert!(app.game().context.ticks.is_empty());
            assert_eq!(app.frame_report(4.0 / 64.0).is_err(), outcome.is_err());
            let report = app.last_frame_report().unwrap();
            assert_eq!(report.scheduled_updates, 4);
            let expected = if outcome == Ok(SystemControl::Continue) {
                4
            } else {
                1
            };
            assert_eq!(report.executed_updates, expected);
            assert_eq!(app.session().tick(), 4);
            assert_eq!(app.game().context.ticks.len(), expected as usize);
            for (index, tick) in app.game().context.ticks.iter().enumerate() {
                assert_eq!(
                    *tick,
                    TickContext {
                        tick: index as u64 + 1,
                        batch_index: index as u32,
                        dt: 1.0 / 64.0
                    }
                );
            }
        }
    }
}
