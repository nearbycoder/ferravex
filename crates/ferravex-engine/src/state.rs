//! Deferred lifecycle transitions (loading, running, paused, etc.) with an explicit
//! allow-list. Hosts handle enter/exit work after commit; callbacks only run when
//! explicitly supplied to [`StateMachine::commit_with`](crate::state::StateMachine::commit_with) or
//! [`StateMachine::try_commit_with`](crate::state::StateMachine::try_commit_with). No state-specific game rules are imposed.
//!
//! ```
//! use ferravex_engine::state::{StateMachine, TransitionError};
//!
//! let mut states = StateMachine::new("loading");
//! assert!(states.allow("loading", "running"));
//! states.check_request(&"running")?;
//! states.request("running")?;
//! assert_eq!(states.pending_transition().unwrap().from, &"loading");
//!
//! let mut lifecycle = Vec::new();
//! states.commit_with(|transition| {
//!     lifecycle.push(("exit", transition.from));
//!     lifecycle.push(("enter", transition.to));
//! });
//! assert_eq!(lifecycle, [("exit", "loading"), ("enter", "running")]);
//! assert_eq!(states.current(), &"running");
//! assert!(states.commit_with(|_| unreachable!()).is_none());
//! # Ok::<(), TransitionError>(())
//! ```
//!
//! Optional tracing records successful commits, including those made through
//! guarded/hooked APIs. Enable it with [`StateMachine::set_trace_capacity`](crate::state::StateMachine::set_trace_capacity); use
//! [`StateMachine::request_with_cause`](crate::state::StateMachine::request_with_cause) to attach a static diagnostic label.
//! Tracing reads no clocks and does not drive simulation or lifecycle effects.
use std::{
    collections::{BTreeSet, TryReserveError, VecDeque, vec_deque},
    fmt,
};

/// Why a request was rejected. Rejection changes neither current nor pending state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    /// The requested target is already current; self-edges never cause re-entry.
    SameState,
    /// No directed edge from the current state to the target has been registered.
    Forbidden,
    /// A transition is already queued; commit, cancel, or explicitly replace it.
    Pending,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SameState => {
                "target is already the current state; choose a different target (self-transitions do not re-enter)"
            }
            Self::Forbidden => {
                "transition is not allowed; register the directed current-to-target edge with StateMachine::allow"
            }
            Self::Pending => {
                "a transition is pending; call commit, cancel, or replace_pending before requesting another"
            }
        })
    }
}

impl std::error::Error for TransitionError {}

/// A transition's endpoints, owned after commit or borrowed during inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition<S> {
    pub from: S,
    pub to: S,
}

/// An owned observation of a committed transition, retained in commit order.
///
/// See [`StateMachine::set_trace_capacity`] for a complete usage example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRecord<S> {
    /// Machine-wide successful commit count, starting at one and saturating at
    /// [`u64::MAX`]. Includes commits made while tracing was disabled.
    pub sequence: u64,
    /// Request label, or `None` for an unlabeled request/replacement.
    pub cause: Option<&'static str>,
    /// Owned copies of the old and new states.
    pub transition: Transition<S>,
}

/// A bounded directed transition graph with one deferred request at a time.
///
/// Requesting, inspecting, replacing, and cancelling never change [`Self::current`].
/// Only a successful commit changes it, and each queued transition is consumed once.
/// Tracing is disabled initially; enabling it retains at most the configured
/// number of records, evicting the oldest on overflow.
pub struct StateMachine<S: Ord + Clone> {
    current: S,
    pending: Option<S>,
    pending_cause: Option<&'static str>,
    allowed: BTreeSet<(S, S)>,
    commit_sequence: u64,
    trace: VecDeque<TransitionRecord<S>>,
    trace_capacity: usize,
}
impl<S: Ord + Clone> StateMachine<S> {
    /// Start at `initial` with no permitted edges, pending transition, or tracing.
    pub fn new(initial: S) -> Self {
        Self {
            current: initial,
            pending: None,
            pending_cause: None,
            allowed: BTreeSet::new(),
            commit_sequence: 0,
            trace: VecDeque::new(),
            trace_capacity: 0,
        }
    }

    /// Enable or resize the commit trace, retaining the newest `capacity` records.
    ///
    /// Zero disables tracing, discards history and releases its backing buffer.
    /// Other sizes reserve the full record capacity up front, so recording never
    /// grows the ring. Shrinking to a nonzero size reuses the existing allocation.
    /// State clones can allocate; the bound is on record count, not state bytes.
    /// Disabled tracing does not clone states beyond the normal commit clone.
    ///
    /// Allocation failure or an unrepresentable capacity returns an error without
    /// changing the trace or its limit. Configuration never changes the current
    /// state, pending request/cause, or [`Self::commit_sequence`]. Enabling tracing
    /// does not recover earlier commits.
    ///
    /// ```
    /// use ferravex_engine::state::{StateMachine, Transition, TransitionRecord};
    /// let mut states = StateMachine::new("menu");
    /// states.allow("menu", "playing");
    /// states.allow("playing", "menu");
    /// states.set_trace_capacity(2)?;
    /// states.request_with_cause("playing", "start button")?;
    /// states.commit_with(|_| {});
    /// assert_eq!(states.trace().next(), Some(&TransitionRecord {
    ///     sequence: 1,
    ///     cause: Some("start button"),
    ///     transition: Transition { from: "menu", to: "playing" },
    /// }));
    /// states.request("menu")?;
    /// states.commit();
    /// states.set_trace_capacity(1)?;
    /// assert_eq!(states.trace_capacity(), 1);
    /// assert_eq!(states.trace().len(), 1);
    /// assert_eq!(states.trace().next().unwrap().sequence, 2);
    /// assert_eq!(states.trace().next().unwrap().cause, None);
    /// states.clear_trace();
    /// assert_eq!(states.trace().len(), 0);
    /// assert_eq!(states.commit_sequence(), 2);
    /// states.set_trace_capacity(0)?;
    /// assert_eq!(states.trace_capacity(), 0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set_trace_capacity(&mut self, capacity: usize) -> Result<(), TryReserveError> {
        self.trace
            .try_reserve_exact(capacity.saturating_sub(self.trace.len()))?;
        while self.trace.len() > capacity {
            self.trace.pop_front();
        }
        if capacity == 0 {
            self.trace = VecDeque::new();
        }
        self.trace_capacity = capacity;
        Ok(())
    }

    /// Maximum retained records; zero means tracing is disabled.
    pub fn trace_capacity(&self) -> usize {
        self.trace_capacity
    }

    /// Borrow records oldest to newest without allocating, cloning or consuming.
    ///
    /// Records are appended before lifecycle hooks run. A record means the state
    /// committed, not that host exit/enter work completed successfully.
    pub fn trace(&self) -> vec_deque::Iter<'_, TransitionRecord<S>> {
        self.trace.iter()
    }

    /// Discard history, retaining capacity, pending context and commit sequence.
    pub fn clear_trace(&mut self) {
        self.trace.clear();
    }

    /// Successful commits since construction, even while tracing is disabled.
    ///
    /// Starts at zero and saturates at [`u64::MAX`]; at saturation, records share
    /// that sequence and iterator order still reflects commit order. Rejected or
    /// cancelled requests, rejected guards and empty commits never advance it.
    pub fn commit_sequence(&self) -> u64 {
        self.commit_sequence
    }

    /// Allow a directed edge. Returns `false` when 256 distinct edges already exist.
    ///
    /// Duplicate edge registration is idempotent. A registered self-edge still
    /// cannot be requested: same-state requests never trigger exit/enter handling.
    pub fn allow(&mut self, from: S, to: S) -> bool {
        let edge = (from, to);
        if self.allowed.contains(&edge) {
            return true;
        }
        if self.allowed.len() == 256 {
            return false;
        }
        self.allowed.insert(edge);
        true
    }
    /// Check whether [`Self::request`] would accept `next`, without queuing it.
    ///
    /// An existing pending transition takes precedence over other errors, exactly
    /// as in `request`. Use [`Self::replace_pending`] to explicitly supersede it.
    pub fn check_request(&self, next: &S) -> Result<(), TransitionError> {
        if self.pending.is_some() {
            return Err(TransitionError::Pending);
        }
        self.check_edge(next)
    }

    fn check_edge(&self, next: &S) -> Result<(), TransitionError> {
        if &self.current == next {
            return Err(TransitionError::SameState);
        }
        if !self.allowed.contains(&(self.current.clone(), next.clone())) {
            return Err(TransitionError::Forbidden);
        }
        Ok(())
    }

    /// Queue `next` without changing the current state or running lifecycle work.
    ///
    /// Any error preserves both the current state and an existing pending request.
    /// The accepted request has no cause label; see [`Self::request_with_cause`].
    pub fn request(&mut self, next: S) -> Result<(), TransitionError> {
        self.check_request(&next)?;
        self.pending = Some(next);
        self.pending_cause = None;
        Ok(())
    }

    /// Queue `next` with a diagnostic cause using the same checks as [`Self::request`].
    ///
    /// The label is retained with the pending request, including across guard
    /// rejection, even while tracing is disabled. Errors preserve the previous
    /// request and cause. Static labels require no allocation or string copying;
    /// they do not affect validation. See [`Self::set_trace_capacity`] for usage.
    pub fn request_with_cause(
        &mut self,
        next: S,
        cause: &'static str,
    ) -> Result<(), TransitionError> {
        self.request(next)?;
        self.pending_cause = Some(cause);
        Ok(())
    }

    /// Atomically queue `next`, returning the previous pending target, if any.
    ///
    /// Validation uses the **current** state, not the old pending target. On an
    /// invalid or same-state target, the previous request is retained. With no
    /// pending request this behaves like [`Self::request`]. Replacing a request
    /// with the same pending target is allowed and does not cause extra lifecycle
    /// work; only a subsequent commit does. Successful replacement clears the old
    /// cause label; use [`Self::replace_pending_with_cause`] to supply a new one.
    ///
    /// ```
    /// use ferravex_engine::state::{StateMachine, TransitionError};
    /// let mut states = StateMachine::new("menu");
    /// states.allow("menu", "playing");
    /// states.allow("menu", "quit");
    /// states.request("playing")?;
    /// assert_eq!(states.replace_pending("quit")?, Some("playing"));
    /// assert_eq!(states.replace_pending("menu"), Err(TransitionError::SameState));
    /// assert_eq!(states.pending(), Some(&"quit"));
    /// assert_eq!(states.current(), &"menu");
    /// # Ok::<(), TransitionError>(())
    /// ```
    pub fn replace_pending(&mut self, next: S) -> Result<Option<S>, TransitionError> {
        self.check_edge(&next)?;
        self.pending_cause = None;
        Ok(self.pending.replace(next))
    }

    /// Atomically replace the queued target and its diagnostic cause.
    ///
    /// Uses [`Self::replace_pending`] validation; errors preserve both the old
    /// target and cause. Replacing with the same pending target updates the label.
    /// With no pending target, queues a labeled request. Neither path records a
    /// transition until commit.
    ///
    /// ```
    /// use ferravex_engine::state::{StateMachine, TransitionError};
    /// let mut states = StateMachine::new("menu");
    /// states.allow("menu", "playing");
    /// states.allow("menu", "quit");
    /// states.request_with_cause("playing", "start button")?;
    /// assert_eq!(states.replace_pending_with_cause("quit", "window closed")?,
    ///     Some("playing"));
    /// assert_eq!(states.pending_cause(), Some("window closed"));
    /// assert_eq!(states.replace_pending_with_cause("menu", "invalid"),
    ///     Err(TransitionError::SameState));
    /// assert_eq!(states.pending_cause(), Some("window closed"));
    /// states.cancel();
    /// assert_eq!(states.pending_cause(), None);
    /// # Ok::<(), TransitionError>(())
    /// ```
    pub fn replace_pending_with_cause(
        &mut self,
        next: S,
        cause: &'static str,
    ) -> Result<Option<S>, TransitionError> {
        let previous = self.replace_pending(next)?;
        self.pending_cause = Some(cause);
        Ok(previous)
    }

    /// The live state, unchanged until a transition is committed.
    pub fn current(&self) -> &S {
        &self.current
    }

    /// The queued target, if any.
    pub fn pending(&self) -> Option<&S> {
        self.pending.as_ref()
    }

    /// Cause of the pending request; `None` if unlabeled or no request is queued.
    pub fn pending_cause(&self) -> Option<&'static str> {
        self.pending_cause
    }

    /// Borrow both endpoints without consuming the request or cloning states.
    pub fn pending_transition(&self) -> Option<Transition<&S>> {
        Some(Transition {
            from: &self.current,
            to: self.pending.as_ref()?,
        })
    }

    /// Discard the pending cause and return the queued target without changing
    /// the live state, commit sequence or history.
    pub fn cancel(&mut self) -> Option<S> {
        self.pending_cause = None;
        self.pending.take()
    }

    /// Consume a request and return its endpoints exactly once, without callbacks.
    ///
    /// The current state has changed and the pending slot is empty before this
    /// returns. Calling again without another request returns `None`. A successful
    /// commit advances [`Self::commit_sequence`] and appends one record if tracing
    /// is enabled, evicting the oldest record when full.
    ///
    /// State clones are prepared before mutation; if cloning panics, the current
    /// state, pending request/cause, sequence and trace remain unchanged.
    pub fn commit(&mut self) -> Option<Transition<S>> {
        let next = self.pending.as_ref()?;
        let current = next.clone();
        let sequence = self.commit_sequence.saturating_add(1);
        let record = (self.trace_capacity != 0).then(|| TransitionRecord {
            sequence,
            cause: self.pending_cause,
            transition: Transition {
                from: self.current.clone(),
                to: next.clone(),
            },
        });
        let to = self.pending.take()?;
        let from = std::mem::replace(&mut self.current, current);
        self.pending_cause = None;
        self.commit_sequence = sequence;
        if let Some(record) = record {
            let evicted = if self.trace.len() == self.trace_capacity {
                self.trace.pop_front()
            } else {
                None
            };
            self.trace.push_back(record);
            drop(evicted);
        }
        Some(Transition { from, to })
    }

    /// Commit, then call `apply` once with the old and new states.
    ///
    /// Put exit handling followed by enter handling in this single closure so
    /// both can borrow the same host context. Nothing runs without a pending
    /// request. `apply` runs **after** the state changes and the request is consumed;
    /// if it panics, the transition remains committed and will not be applied again.
    /// Its trace record (when enabled) is retained even if `apply` panics.
    /// Use [`Self::try_commit_with`] for fallible checks before lifecycle effects.
    pub fn commit_with(&mut self, apply: impl FnOnce(&Transition<S>)) -> Option<Transition<S>> {
        let transition = self.commit()?;
        apply(&transition);
        Some(transition)
    }

    /// Check a pending transition, then commit and run lifecycle work once.
    ///
    /// `guard` borrows the endpoints before any state change. On `Err`, the current
    /// state and pending target are retained, and `apply` is not called. The caller
    /// can retry, cancel, or replace that request. With no request, neither closure
    /// runs and this returns `Ok(None)`. Errors are returned unchanged.
    /// Rejection also retains the pending cause and leaves sequence/history intact.
    ///
    /// As with [`Self::commit_with`], `apply` runs after commit. Keep exit/enter
    /// effects there: changes to external data made by `guard` cannot be rolled back.
    ///
    /// ```
    /// use ferravex_engine::state::StateMachine;
    /// let mut states = StateMachine::new("loading");
    /// states.allow("loading", "running");
    /// states.request("running").unwrap();
    /// let assets_ready = false;
    /// let result = states.try_commit_with(
    ///     |_| if assets_ready { Ok(()) } else { Err("load the level assets first") },
    ///     |_| unreachable!("a rejected transition has no lifecycle effects"),
    /// );
    /// assert_eq!(result, Err("load the level assets first"));
    /// assert_eq!(states.current(), &"loading");
    /// assert_eq!(states.pending(), Some(&"running"));
    ///
    /// let mut lifecycle = Vec::new();
    /// states.try_commit_with(
    ///     |_| Ok::<(), &str>(()),
    ///     |transition| {
    ///         lifecycle.push(("exit", transition.from));
    ///         lifecycle.push(("enter", transition.to));
    ///     },
    /// ).unwrap();
    /// assert_eq!(lifecycle, [("exit", "loading"), ("enter", "running")]);
    /// ```
    pub fn try_commit_with<E>(
        &mut self,
        guard: impl FnOnce(Transition<&S>) -> Result<(), E>,
        apply: impl FnOnce(&Transition<S>),
    ) -> Result<Option<Transition<S>>, E> {
        let Some(transition) = self.pending_transition() else {
            return Ok(None);
        };
        guard(transition)?;
        Ok(self.commit_with(apply))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_edges_and_pending_transitions_never_mutate_live_state() {
        let mut s = StateMachine::new("loading");
        s.allow("loading", "running");
        s.allow("running", "paused");
        assert_eq!(s.request("paused"), Err(TransitionError::Forbidden));
        assert_eq!(s.request("loading"), Err(TransitionError::SameState));
        s.request("running").unwrap();
        assert_eq!(s.current(), &"loading");
        assert_eq!(s.request("running"), Err(TransitionError::Pending));
        assert_eq!(
            s.commit(),
            Some(Transition {
                from: "loading",
                to: "running"
            })
        );
        assert_eq!(s.commit(), None);
        s.request("paused").unwrap();
        assert_eq!(s.cancel(), Some("paused"));
        assert_eq!(s.current(), &"running");
        assert_eq!(s.pending(), None);
    }

    #[test]
    fn request_inspection_matches_request_errors_without_queuing() {
        let mut states = StateMachine::new("menu");
        states.allow("menu", "playing");
        assert_eq!(states.check_request(&"playing"), Ok(()));
        assert_eq!(
            states.check_request(&"menu"),
            Err(TransitionError::SameState)
        );
        assert_eq!(
            states.check_request(&"quit"),
            Err(TransitionError::Forbidden)
        );
        assert_eq!(states.current(), &"menu");
        assert_eq!(states.pending_transition(), None);

        states.request("playing").unwrap();
        for next in ["menu", "playing", "quit"] {
            assert_eq!(states.check_request(&next), Err(TransitionError::Pending));
            assert_eq!(states.request(next), Err(TransitionError::Pending));
            assert_eq!(states.current(), &"menu");
            assert_eq!(states.pending(), Some(&"playing"));
        }
    }

    #[test]
    fn transition_inspection_borrows_live_endpoints_without_consuming() {
        let mut states = StateMachine::new(String::from("menu"));
        states.allow(String::from("menu"), String::from("playing"));
        states.request(String::from("playing")).unwrap();

        for _ in 0..2 {
            let pending = states.pending_transition().unwrap();
            assert!(std::ptr::eq(pending.from, states.current()));
            assert!(std::ptr::eq(pending.to, states.pending().unwrap()));
            assert_eq!(pending.from, "menu");
            assert_eq!(pending.to, "playing");
        }
        let committed = states.commit().unwrap();
        assert_eq!(committed.from, "menu");
        assert_eq!(committed.to, "playing");
        assert_eq!(states.pending_transition(), None);
    }

    #[test]
    fn pending_replacement_validates_from_current_and_preserves_rejected_request() {
        let mut states = StateMachine::new("menu");
        states.allow("menu", "playing");
        states.allow("menu", "quit");
        states.allow("playing", "paused");
        states.request("playing").unwrap();

        assert_eq!(
            states.replace_pending("menu"),
            Err(TransitionError::SameState)
        );
        assert_eq!(states.pending(), Some(&"playing"));
        // The pending state's outgoing edge cannot authorize a replacement.
        assert_eq!(
            states.replace_pending("paused"),
            Err(TransitionError::Forbidden)
        );
        assert_eq!(states.pending(), Some(&"playing"));
        assert_eq!(states.replace_pending("playing"), Ok(Some("playing")));
        assert_eq!(states.replace_pending("quit"), Ok(Some("playing")));
        assert_eq!(states.current(), &"menu");

        let mut lifecycle = Vec::new();
        states.commit_with(|transition| {
            lifecycle.push(("exit", transition.from));
            lifecycle.push(("enter", transition.to));
        });
        assert_eq!(lifecycle, [("exit", "menu"), ("enter", "quit")]);
        assert_eq!(states.current(), &"quit");
        assert!(
            states
                .commit_with(|_| panic!("transition ran twice"))
                .is_none()
        );
    }

    #[test]
    fn replacement_without_a_pending_request_queues_only_valid_targets() {
        let mut states = StateMachine::new("menu");
        states.allow("menu", "playing");
        assert_eq!(
            states.replace_pending("menu"),
            Err(TransitionError::SameState)
        );
        assert_eq!(
            states.replace_pending("quit"),
            Err(TransitionError::Forbidden)
        );
        assert_eq!(states.current(), &"menu");
        assert_eq!(states.pending(), None);
        assert_eq!(states.replace_pending("playing"), Ok(None));
        assert_eq!(states.current(), &"menu");
        assert_eq!(states.pending(), Some(&"playing"));
    }

    #[test]
    fn cancel_same_state_and_invalid_requests_never_run_lifecycle_work() {
        let mut states = StateMachine::new("menu");
        states.allow("menu", "playing");
        states.allow("menu", "menu");
        assert_eq!(states.request("menu"), Err(TransitionError::SameState));
        assert_eq!(states.request("quit"), Err(TransitionError::Forbidden));
        assert!(
            states
                .commit_with(|_| panic!("rejected request was applied"))
                .is_none()
        );

        states.request("playing").unwrap();
        assert_eq!(states.cancel(), Some("playing"));
        assert_eq!(states.cancel(), None);
        let result = states.try_commit_with::<()>(
            |_| panic!("cancelled request was checked"),
            |_| panic!("cancelled request was applied"),
        );
        assert_eq!(result, Ok(None));
        assert_eq!(states.current(), &"menu");
        assert_eq!(states.pending_transition(), None);

        states.request("playing").unwrap();
        assert!(states.commit_with(|_| {}).is_some());
        assert_eq!(states.current(), &"playing");
    }

    #[test]
    fn guard_rejection_retains_request_for_retry_and_applies_lifecycle_once() {
        #[derive(Debug, PartialEq, Eq)]
        struct MissingAsset(&'static str);

        let mut states = StateMachine::new("loading");
        states.allow("loading", "running");
        states.request("running").unwrap();
        let mut guard_calls = 0;
        let mut lifecycle = Vec::new();
        let result = states.try_commit_with(
            |transition| {
                guard_calls += 1;
                assert_eq!(transition.from, &"loading");
                assert_eq!(transition.to, &"running");
                Err(MissingAsset("level"))
            },
            |transition| lifecycle.push(("unexpected", transition.to)),
        );
        assert_eq!(result, Err(MissingAsset("level")));
        assert_eq!(guard_calls, 1);
        assert!(lifecycle.is_empty());
        assert_eq!(states.current(), &"loading");
        assert_eq!(states.pending(), Some(&"running"));

        let result = states.try_commit_with(
            |_| {
                guard_calls += 1;
                Ok::<(), MissingAsset>(())
            },
            |transition| {
                lifecycle.push(("exit", transition.from));
                lifecycle.push(("enter", transition.to));
            },
        );
        assert_eq!(
            result,
            Ok(Some(Transition {
                from: "loading",
                to: "running"
            }))
        );
        assert_eq!(guard_calls, 2);
        assert_eq!(lifecycle, [("exit", "loading"), ("enter", "running")]);
        assert_eq!(states.current(), &"running");
        assert_eq!(states.pending(), None);
        assert_eq!(
            states.try_commit_with::<MissingAsset>(
                |_| panic!("empty request was checked"),
                |_| panic!("committed request was applied twice"),
            ),
            Ok(None)
        );
    }

    #[test]
    fn applied_transitions_are_not_repeated_but_later_requests_are_applied() {
        let mut states = StateMachine::new(String::from("running"));
        states.allow(String::from("running"), String::from("paused"));
        states.allow(String::from("paused"), String::from("running"));
        let mut lifecycle = Vec::new();
        for next in ["paused", "running"] {
            states.request(String::from(next)).unwrap();
            states.commit_with(|transition| {
                lifecycle.push(("exit", transition.from.clone()));
                lifecycle.push(("enter", transition.to.clone()));
            });
            assert!(
                states
                    .commit_with(|_| panic!("transition repeated"))
                    .is_none()
            );
        }
        assert_eq!(
            lifecycle,
            [
                ("exit", String::from("running")),
                ("enter", String::from("paused")),
                ("exit", String::from("paused")),
                ("enter", String::from("running")),
            ]
        );
        assert_eq!(states.current(), "running");
    }

    #[test]
    fn lifecycle_panic_leaves_transition_committed_and_consumed() {
        let mut states = StateMachine::new("loading");
        states.allow("loading", "running");
        states.request("running").unwrap();
        let mut apply_calls = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            states.commit_with(|_| {
                apply_calls += 1;
                panic!("host lifecycle failed");
            });
        }));
        assert!(result.is_err());
        assert_eq!(apply_calls, 1);
        assert_eq!(states.current(), &"running");
        assert_eq!(states.pending(), None);
        assert!(
            states
                .commit_with(|_| panic!("panicked handler repeated"))
                .is_none()
        );
    }

    #[test]
    fn transition_errors_explain_recovery_and_implement_std_error() {
        for (error, recovery) in [
            (TransitionError::SameState, "different target"),
            (TransitionError::Forbidden, "StateMachine::allow"),
            (TransitionError::Pending, "replace_pending"),
        ] {
            let diagnostic: &dyn std::error::Error = &error;
            assert!(diagnostic.to_string().contains(recovery));
            assert!(diagnostic.source().is_none());
        }
    }

    #[test]
    fn trace_observes_each_commit_path_once_and_preserves_guard_context() {
        let mut states = StateMachine::new("loading");
        states.allow("loading", "running");
        states.allow("running", "paused");
        states.allow("paused", "running");
        states.set_trace_capacity(3).unwrap();
        states
            .request_with_cause("running", "assets ready")
            .unwrap();
        assert_eq!(
            states.try_commit_with(|_| Err("not ready"), |_| unreachable!()),
            Err("not ready")
        );
        assert_eq!(states.current(), &"loading");
        assert_eq!(states.pending(), Some(&"running"));
        assert_eq!(states.pending_cause(), Some("assets ready"));
        assert_eq!(states.commit_sequence(), 0);
        assert_eq!(states.trace().len(), 0);

        let mut hooks = Vec::new();
        states
            .try_commit_with(
                |_| Ok::<(), &str>(()),
                |transition| hooks.push(transition.clone()),
            )
            .unwrap();
        assert_eq!(states.pending_cause(), None);
        assert_eq!(
            states.try_commit_with::<()>(|_| unreachable!(), |_| unreachable!()),
            Ok(None)
        );
        states.request_with_cause("paused", "pause button").unwrap();
        states.commit_with(|transition| hooks.push(transition.clone()));
        assert_eq!(states.commit_with(|_| unreachable!()), None);
        states.request("running").unwrap();
        states.commit();
        assert_eq!(states.commit(), None);

        let expected = [
            TransitionRecord {
                sequence: 1,
                cause: Some("assets ready"),
                transition: Transition {
                    from: "loading",
                    to: "running",
                },
            },
            TransitionRecord {
                sequence: 2,
                cause: Some("pause button"),
                transition: Transition {
                    from: "running",
                    to: "paused",
                },
            },
            TransitionRecord {
                sequence: 3,
                cause: None,
                transition: Transition {
                    from: "paused",
                    to: "running",
                },
            },
        ];
        for _ in 0..2 {
            assert!(states.trace().eq(expected.iter()));
        }
        assert_eq!(
            hooks,
            [
                expected[0].transition.clone(),
                expected[1].transition.clone()
            ]
        );
        assert_eq!(states.commit_sequence(), 3);
    }

    #[test]
    fn trace_cause_tracks_only_accepted_requests_and_replacements() {
        let mut states = StateMachine::new("menu");
        states.allow("menu", "playing");
        states.allow("menu", "quit");
        states.allow("playing", "paused");
        states.set_trace_capacity(2).unwrap();
        assert_eq!(
            states.request_with_cause("menu", "invalid"),
            Err(TransitionError::SameState)
        );
        assert_eq!(
            states.request_with_cause("paused", "invalid"),
            Err(TransitionError::Forbidden)
        );
        assert_eq!(states.pending_cause(), None);
        states
            .request_with_cause("playing", "start button")
            .unwrap();
        for next in ["menu", "playing", "quit", "paused"] {
            assert_eq!(
                states.request_with_cause(next, "rejected"),
                Err(TransitionError::Pending)
            );
            assert_eq!(states.request(next), Err(TransitionError::Pending));
            assert_eq!(states.pending_cause(), Some("start button"));
        }
        for (next, error) in [
            ("menu", TransitionError::SameState),
            ("paused", TransitionError::Forbidden),
        ] {
            assert_eq!(
                states.replace_pending_with_cause(next, "invalid"),
                Err(error)
            );
            assert_eq!(states.replace_pending(next), Err(error));
            assert_eq!(states.pending(), Some(&"playing"));
            assert_eq!(states.pending_cause(), Some("start button"));
        }
        assert_eq!(
            states.replace_pending_with_cause("playing", "AI start"),
            Ok(Some("playing"))
        );
        assert_eq!(states.pending_cause(), Some("AI start"));
        assert_eq!(states.replace_pending("playing"), Ok(Some("playing")));
        assert_eq!(states.pending_cause(), None);
        assert_eq!(
            states.replace_pending_with_cause("quit", "window closed"),
            Ok(Some("playing"))
        );
        assert_eq!(states.trace().len(), 0);
        assert_eq!(states.commit_sequence(), 0);
        let mut calls = 0;
        states.commit_with(|_| calls += 1);
        assert_eq!(calls, 1);
        assert_eq!(states.trace().len(), 1);
        let record = states.trace().next().unwrap();
        assert_eq!(record.cause, Some("window closed"));
        assert_eq!(
            record.transition,
            Transition {
                from: "menu",
                to: "quit"
            }
        );
    }

    #[test]
    fn cancelled_or_unlabeled_replacements_do_not_leak_causes() {
        let mut states = StateMachine::new(0);
        states.allow(0, 1);
        states.allow(1, 0);
        states.set_trace_capacity(3).unwrap();
        assert_eq!(states.replace_pending_with_cause(1, "cancel me"), Ok(None));
        assert_eq!(states.cancel(), Some(1));
        assert_eq!(states.cancel(), None);
        assert_eq!(states.pending_cause(), None);
        assert_eq!(states.commit(), None);
        assert_eq!(states.commit_sequence(), 0);
        assert_eq!(states.trace().len(), 0);

        states.request(1).unwrap();
        states.commit();
        states.request_with_cause(0, "replace me").unwrap();
        assert_eq!(states.replace_pending(0), Ok(Some(0)));
        states.commit();
        assert_eq!(
            states.replace_pending_with_cause(1, "new request"),
            Ok(None)
        );
        states.commit();
        assert_eq!(
            states
                .trace()
                .map(|record| record.cause)
                .collect::<Vec<_>>(),
            [None, None, Some("new request")]
        );
    }

    #[test]
    fn trace_rolls_over_resizes_clears_and_reenables_without_resetting_sequence() {
        let mut states = StateMachine::new(String::from("a"));
        states.allow(String::from("a"), String::from("b"));
        states.allow(String::from("b"), String::from("a"));
        assert_eq!(states.trace_capacity(), 0);
        assert_eq!(states.trace.capacity(), 0);
        states.request(String::from("b")).unwrap();
        states.commit();
        assert_eq!(states.trace().len(), 0);
        assert_eq!(states.commit_sequence(), 1);

        states.set_trace_capacity(3).unwrap();
        let reserved = states.trace.capacity();
        for next in ["a", "b", "a", "b", "a", "b"] {
            states
                .request_with_cause(String::from(next), "step")
                .unwrap();
            states.commit();
            assert!(states.trace().len() <= 3);
            assert_eq!(states.trace.capacity(), reserved);
        }
        assert_eq!(
            states
                .trace()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            [5, 6, 7]
        );
        assert_eq!(
            states
                .trace()
                .map(|record| (
                    record.transition.from.as_str(),
                    record.transition.to.as_str()
                ))
                .collect::<Vec<_>>(),
            [("a", "b"), ("b", "a"), ("a", "b")]
        );
        states.set_trace_capacity(1).unwrap();
        assert_eq!(states.trace().len(), 1);
        assert_eq!(states.trace().next().unwrap().sequence, 7);
        states.set_trace_capacity(4).unwrap();
        assert_eq!(states.trace().len(), 1);
        states
            .request_with_cause(String::from("a"), "pending")
            .unwrap();
        states.clear_trace();
        assert_eq!(states.trace().len(), 0);
        assert_eq!(states.trace_capacity(), 4);
        assert_eq!(states.commit_sequence(), 7);
        assert_eq!(states.pending_cause(), Some("pending"));
        states.commit();
        assert_eq!(states.trace().next().unwrap().sequence, 8);
        states.set_trace_capacity(0).unwrap();
        assert_eq!(states.trace().len(), 0);
        assert_eq!(states.trace.capacity(), 0);
        states.request(String::from("b")).unwrap();
        states.commit();
        assert_eq!(states.commit_sequence(), 9);
        assert_eq!(states.trace().len(), 0);

        states
            .request_with_cause(String::from("a"), "while disabled")
            .unwrap();
        states.set_trace_capacity(1).unwrap();
        states.commit();
        let record = states.trace().next().unwrap();
        assert_eq!(record.sequence, 10);
        assert_eq!(record.cause, Some("while disabled"));
        states.request(String::from("b")).unwrap();
        states.commit();
        assert_eq!(states.trace().len(), 1);
        assert_eq!(states.trace().next().unwrap().sequence, 11);
    }

    #[test]
    fn impossible_trace_capacity_preserves_existing_history_and_pending_context() {
        let mut states = StateMachine::new(0);
        states.allow(0, 1);
        states.allow(1, 0);
        states.set_trace_capacity(1).unwrap();
        states.request_with_cause(1, "first").unwrap();
        states.commit();
        states.request_with_cause(0, "second").unwrap();
        let before = states.trace().cloned().collect::<Vec<_>>();
        assert!(states.set_trace_capacity(usize::MAX).is_err());
        assert_eq!(states.trace_capacity(), 1);
        assert!(states.trace().eq(before.iter()));
        assert_eq!(states.current(), &1);
        assert_eq!(states.pending(), Some(&0));
        assert_eq!(states.pending_cause(), Some("second"));
        assert_eq!(states.commit_sequence(), 1);
        states.commit();
        assert_eq!(states.trace().next().unwrap().sequence, 2);
        assert_eq!(states.trace().next().unwrap().cause, Some("second"));
    }

    #[test]
    fn trace_survives_hook_panics_but_never_records_a_panicking_guard() {
        let mut states = StateMachine::new(0);
        states.allow(0, 1);
        states.set_trace_capacity(1).unwrap();
        states.request_with_cause(1, "start").unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            states.try_commit_with::<()>(|_| panic!("guard failed"), |_| unreachable!())
        }));
        assert!(result.is_err());
        assert_eq!(states.current(), &0);
        assert_eq!(states.pending(), Some(&1));
        assert_eq!(states.pending_cause(), Some("start"));
        assert_eq!(states.commit_sequence(), 0);
        assert_eq!(states.trace().len(), 0);

        let mut calls = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            states.try_commit_with(
                |_| Ok::<(), ()>(()),
                |_| {
                    calls += 1;
                    panic!("host lifecycle failed");
                },
            )
        }));
        assert!(result.is_err());
        assert_eq!(calls, 1);
        assert_eq!(states.current(), &1);
        assert_eq!(states.pending(), None);
        assert_eq!(states.pending_cause(), None);
        assert_eq!(states.commit_sequence(), 1);
        assert_eq!(states.trace().len(), 1);
        assert_eq!(states.trace().next().unwrap().cause, Some("start"));
        assert_eq!(states.commit_with(|_| unreachable!()), None);
        assert_eq!(states.trace().len(), 1);
    }

    #[test]
    fn trace_sequence_saturates_without_blocking_commits_or_hooks() {
        let mut states = StateMachine::new(0);
        states.allow(0, 1);
        states.allow(1, 0);
        states.set_trace_capacity(2).unwrap();
        states.commit_sequence = u64::MAX - 1;
        let mut hooks = 0;
        for next in [1, 0] {
            states.request(next).unwrap();
            states.commit_with(|_| hooks += 1);
        }
        assert_eq!(hooks, 2);
        assert_eq!(states.commit_sequence(), u64::MAX);
        assert_eq!(
            states
                .trace()
                .map(|record| record.sequence)
                .collect::<Vec<_>>(),
            [u64::MAX, u64::MAX]
        );
        assert_eq!(
            states
                .trace()
                .map(|record| record.transition.to)
                .collect::<Vec<_>>(),
            [1, 0]
        );
    }

    #[test]
    fn trace_clones_are_opt_in_and_clone_panics_preserve_pending_requests() {
        use std::cell::Cell;

        thread_local! {
            static CLONES: Cell<usize> = const { Cell::new(0) };
            static PANIC_ON: Cell<usize> = const { Cell::new(0) };
        }
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
        struct Probe(u8);
        impl Clone for Probe {
            fn clone(&self) -> Self {
                let count = CLONES.get() + 1;
                CLONES.set(count);
                assert_ne!(count, PANIC_ON.get(), "state clone failed");
                Self(self.0)
            }
        }

        let mut states = StateMachine::new(Probe(0));
        states.allow(Probe(0), Probe(1));
        states.allow(Probe(1), Probe(0));
        states
            .request_with_cause(Probe(1), "without tracing")
            .unwrap();
        CLONES.set(0);
        states.commit();
        assert_eq!(CLONES.get(), 1);
        assert_eq!(states.trace.capacity(), 0);
        states.set_trace_capacity(1).unwrap();
        states.request_with_cause(Probe(0), "first record").unwrap();
        states.commit();
        states.request_with_cause(Probe(1), "retry").unwrap();

        // Failure at each clone point preserves even the record about to be evicted.
        for fail_at in 1..=3 {
            CLONES.set(0);
            PANIC_ON.set(fail_at);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                states.commit_with(|_| unreachable!("cloning did not complete"))
            }));
            PANIC_ON.set(0);
            assert!(result.is_err());
            assert_eq!(states.current().0, 0);
            assert_eq!(states.pending().unwrap().0, 1);
            assert_eq!(states.pending_cause(), Some("retry"));
            assert_eq!(states.commit_sequence(), 2);
            assert_eq!(states.trace().len(), 1);
            assert_eq!(states.trace().next().unwrap().sequence, 2);
            assert_eq!(states.trace().next().unwrap().cause, Some("first record"));
        }
        CLONES.set(0);
        states.commit();
        assert_eq!(CLONES.get(), 3);
        assert_eq!(states.trace().next().unwrap().sequence, 3);
        assert_eq!(states.trace().next().unwrap().cause, Some("retry"));

        states.set_trace_capacity(0).unwrap();
        states.request(Probe(0)).unwrap();
        CLONES.set(0);
        states.commit();
        assert_eq!(CLONES.get(), 1);
        assert_eq!(states.trace.capacity(), 0);
    }
}
