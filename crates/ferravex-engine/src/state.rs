//! Deferred lifecycle transitions (loading, running, paused, etc.) with an explicit
//! allow-list. Hosts handle enter/exit work after commit; no callbacks run implicitly.
use std::collections::BTreeSet;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    SameState,
    Forbidden,
    Pending,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition<S> {
    pub from: S,
    pub to: S,
}
pub struct StateMachine<S: Ord + Clone> {
    current: S,
    pending: Option<S>,
    allowed: BTreeSet<(S, S)>,
}
impl<S: Ord + Clone> StateMachine<S> {
    pub fn new(initial: S) -> Self {
        Self {
            current: initial,
            pending: None,
            allowed: BTreeSet::new(),
        }
    }
    /// At most 256 distinct edges; duplicate edge registration is idempotent.
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
    pub fn request(&mut self, next: S) -> Result<(), TransitionError> {
        if self.pending.is_some() {
            return Err(TransitionError::Pending);
        }
        if self.current == next {
            return Err(TransitionError::SameState);
        }
        if !self.allowed.contains(&(self.current.clone(), next.clone())) {
            return Err(TransitionError::Forbidden);
        }
        self.pending = Some(next);
        Ok(())
    }
    pub fn current(&self) -> &S {
        &self.current
    }
    pub fn pending(&self) -> Option<&S> {
        self.pending.as_ref()
    }
    pub fn cancel(&mut self) -> Option<S> {
        self.pending.take()
    }
    pub fn commit(&mut self) -> Option<Transition<S>> {
        let to = self.pending.take()?;
        let from = std::mem::replace(&mut self.current, to.clone());
        Some(Transition { from, to })
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
}
