//! Bounded FIFO engine events. Capacity counts events, not their owned payload bytes.
//! Overflow is explicit; retained events preserve send order.
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    RejectNewest,
    EvictOldest,
}
pub struct EventQueue<T> {
    queue: VecDeque<T>,
    capacity: usize,
    overflow: Overflow,
    dropped: u64,
}
impl<T> EventQueue<T> {
    pub fn new(capacity: usize, overflow: Overflow) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity,
            overflow,
            dropped: 0,
        }
    }
    /// Returns the evicted value, or the rejected input as Err. Zero capacity always rejects.
    pub fn send(&mut self, value: T) -> Result<Option<T>, T> {
        if self.queue.len() == self.capacity {
            self.dropped = self.dropped.saturating_add(1);
            if self.capacity == 0 || self.overflow == Overflow::RejectNewest {
                return Err(value);
            }
            let old = self.queue.pop_front();
            self.queue.push_back(value);
            return Ok(old);
        }
        self.queue.push_back(value);
        Ok(None)
    }
    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_fifo_and_returns_owned_overflow() {
        let mut q = EventQueue::new(2, Overflow::RejectNewest);
        assert_eq!(q.send(1), Ok(None));
        q.send(2).unwrap();
        assert_eq!(q.send(3), Err(3));
        assert_eq!((q.pop(), q.pop(), q.pop()), (Some(1), Some(2), None));
        assert_eq!(q.dropped(), 1);
        let mut q = EventQueue::new(2, Overflow::EvictOldest);
        q.send(1).unwrap();
        q.send(2).unwrap();
        assert_eq!(q.send(3), Ok(Some(1)));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(3));
        assert_eq!(EventQueue::new(0, Overflow::EvictOldest).send(7), Err(7));
    }
}
