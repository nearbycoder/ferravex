//! Bounded tick deadlines with deterministic FIFO ties and explicit work limits.
//! IDs belong to their scheduler; tick values are monotonic absolute host ticks.
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskId(u64);
pub struct TickScheduler<T> {
    tasks: BTreeMap<(u64, u64), T>,
    next: u64,
    capacity: usize,
}
impl<T> TickScheduler<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            tasks: BTreeMap::new(),
            next: 0,
            capacity,
        }
    }
    pub fn schedule(&mut self, tick: u64, task: T) -> Result<TaskId, T> {
        if self.tasks.len() >= self.capacity || self.next == u64::MAX {
            return Err(task);
        }
        let id = self.next;
        self.next += 1;
        self.tasks.insert((tick, id), task);
        Ok(TaskId(id))
    }
    pub fn cancel(&mut self, id: TaskId) -> Option<T> {
        let key = self
            .tasks
            .keys()
            .find(|(_, serial)| *serial == id.0)
            .copied()?;
        self.tasks.remove(&key)
    }
    pub fn next_tick(&self) -> Option<u64> {
        self.tasks.first_key_value().map(|((tick, _), _)| *tick)
    }
    /// Drain at most `limit` due tasks; overdue work remains for a subsequent call.
    pub fn drain_ready(&mut self, tick: u64, limit: usize) -> Vec<T> {
        let mut out = Vec::new();
        while out.len() < limit && self.next_tick().is_some_and(|next| next <= tick) {
            out.push(self.tasks.pop_first().unwrap().1);
        }
        out
    }
    pub fn len(&self) -> usize {
        self.tasks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
