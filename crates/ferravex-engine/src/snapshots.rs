//! Bounded monotonic tick snapshots. Hosts own state and interpolation semantics.
//! Failed inserts preserve history; rolling eviction removes only the oldest state.
use std::collections::VecDeque;
pub type SnapshotRef<'a, T> = Option<(u64, &'a T)>;
pub struct SnapshotRing<T> {
    states: VecDeque<(u64, T)>,
    capacity: usize,
}
impl<T> SnapshotRing<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            states: VecDeque::new(),
            capacity,
        }
    }
    pub fn push(&mut self, tick: u64, state: T) -> Result<Option<(u64, T)>, T> {
        if self.capacity == 0 || self.states.back().is_some_and(|(last, _)| *last >= tick) {
            return Err(state);
        }
        let old = if self.states.len() == self.capacity {
            self.states.pop_front()
        } else {
            None
        };
        self.states.push_back((tick, state));
        Ok(old)
    }
    pub fn get(&self, tick: u64) -> Option<&T> {
        self.states.iter().find(|(t, _)| *t == tick).map(|(_, s)| s)
    }
    /// Nearest samples on either side (inclusive); exact matches appear in both slots.
    pub fn bracket(&self, tick: u64) -> (SnapshotRef<'_, T>, SnapshotRef<'_, T>) {
        let before = self
            .states
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(t, s)| (*t, s));
        let after = self
            .states
            .iter()
            .find(|(t, _)| *t >= tick)
            .map(|(t, s)| (*t, s));
        (before, after)
    }
    /// Discard the future so a rollback host can append a new timeline.
    pub fn truncate_after(&mut self, tick: u64) {
        while self.states.back().is_some_and(|(t, _)| *t > tick) {
            self.states.pop_back();
        }
    }
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_brackets_and_rollback_branch() {
        let mut h = SnapshotRing::new(2);
        h.push(10, "a").unwrap();
        h.push(20, "b").unwrap();
        assert_eq!(h.push(19, "bad"), Err("bad"));
        assert_eq!(h.bracket(15), (Some((10, &"a")), Some((20, &"b"))));
        assert_eq!(h.bracket(20), (Some((20, &"b")), Some((20, &"b"))));
        assert_eq!(h.bracket(0), (None, Some((10, &"a"))));
        assert_eq!(h.push(30, "c"), Ok(Some((10, "a"))));
        assert!(h.get(10).is_none());
        h.truncate_after(20);
        h.push(25, "branch").unwrap();
        assert_eq!(h.get(25), Some(&"branch"));
        assert_eq!(h.get(30), None);
        assert_eq!(SnapshotRing::new(0).push(0, 7), Err(7));
    }
}
