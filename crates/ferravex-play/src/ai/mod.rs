//! Bounded, renderer-independent perception, decisions and work scheduling.
//! Hosts supply candidates and authoritative occlusion queries. Deferred sight
//! never counts as a fresh sighting; decisions cannot manufacture combat hits.
mod decision;
mod enemy;
mod perception;
mod scheduler;
mod squad;
pub use decision::*;
pub use enemy::*;
pub use perception::*;
pub use scheduler::*;
pub use squad::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiError {
    InvalidConfiguration,
    InvalidInput,
    Capacity,
    InvalidState,
}

/// Shared per-tick allowances. Counts include rejected work attempts; actual
/// navigation work is charged only after the incremental search consumes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkCounts {
    pub candidates: u32,
    pub queries: u32,
    pub path_steps: u32,
    pub deferred_candidates: u32,
    pub deferred_queries: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct WorkBudget {
    candidates: u32,
    queries: u32,
    paths: u32,
    counts: WorkCounts,
}
impl WorkBudget {
    pub fn new(candidates: u32, queries: u32, path_steps: u32) -> Self {
        Self {
            candidates,
            queries,
            paths: path_steps,
            counts: WorkCounts::default(),
        }
    }
    pub fn counts(&self) -> WorkCounts {
        self.counts
    }
    pub fn remaining_path_steps(&self) -> u32 {
        self.paths
    }
    pub fn spend_path_steps(&mut self, steps: u32) -> bool {
        if steps > self.paths {
            return false;
        }
        self.paths -= steps;
        self.counts.path_steps += steps;
        true
    }
    pub fn candidate(&mut self) -> bool {
        if self.candidates == 0 {
            self.counts.deferred_candidates = self.counts.deferred_candidates.saturating_add(1);
            false
        } else {
            self.candidates -= 1;
            self.counts.candidates += 1;
            true
        }
    }
    pub fn query(&mut self) -> bool {
        if self.queries == 0 {
            self.counts.deferred_queries = self.counts.deferred_queries.saturating_add(1);
            false
        } else {
            self.queries -= 1;
            self.counts.queries += 1;
            true
        }
    }
}
