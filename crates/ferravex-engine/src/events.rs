//! Bounded FIFO engine events. Capacity counts events, not their owned payload bytes.
//! Overflow is explicit; retained events preserve send order.
//! [`EventQueue::try_send_batch`](crate::events::EventQueue::try_send_batch) admits a whole batch only when every event fits,
//! preserving queued events and the caller's batch on capacity rejection.
//!
//! Iteration and draining allocate no storage and do not clone events. A limited
//! drain lets a consumer leave excess work queued for its next tick:
//!
//! ```
//! use ferravex_engine::events::{EventQueue, Overflow};
//!
//! let mut events = EventQueue::new(2, Overflow::RejectNewest);
//! events.send(10).unwrap();
//! events.send(20).unwrap();
//! assert_eq!(events.send(30), Err(30));
//! assert!(events.is_full());
//! assert_eq!(events.metrics().rejected, 1);
//! assert_eq!(events.drain_up_to(1).sum::<i32>(), 10);
//! assert_eq!(events.iter().copied().next(), Some(20));
//! assert_eq!(events.remaining_capacity(), 1);
//! assert_eq!(events.drain().sum::<i32>(), 20);
//! let interval = events.reset_metrics();
//! assert_eq!(interval.high_water_mark, 2);
//! assert_eq!(events.dropped(), 0);
//! ```
use std::{
    collections::{VecDeque, vec_deque},
    fmt,
};

/// The action taken when an individual [`EventQueue::send`] encounters a full queue.
/// Atomic batches always reject if they do not fit, regardless of this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Return the new event to its sender without changing the queue.
    RejectNewest,
    /// Accept the new event and return the oldest queued event to its sender.
    EvictOldest,
}

/// An entire batch was rejected because it exceeded the available event slots.
///
/// [`EventQueue::try_send_batch`] leaves the input vector and queued events
/// unchanged on this error, allowing a retry after enough events are consumed.
/// Its usage example shows both the error fields and a successful retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventBatchError {
    /// Number of events in the rejected batch; none were enqueued.
    pub requested: usize,
    /// Number of free queue slots at the time of rejection.
    pub remaining_capacity: usize,
}

impl fmt::Display for EventBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event batch of {} events exceeds remaining capacity of {} events",
            self.requested, self.remaining_capacity
        )
    }
}

impl std::error::Error for EventBatchError {}

/// Backpressure observed since construction or the last metrics reset.
///
/// Counters saturate at [`u64::MAX`]. Pops, drains and clears do not count as
/// overflow and do not change these metrics. Batches count each event, not each
/// call; a rejected batch counts every attempted event even if retried later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventQueueMetrics {
    /// Events accepted individually or in batches, including evicting sends.
    pub accepted: u64,
    /// Events rejected individually or in batches, including at zero capacity.
    pub rejected: u64,
    /// Previously queued events displaced by accepted sends.
    pub evicted: u64,
    /// Largest queue length observed, including the length at the last reset.
    pub high_water_mark: usize,
}

/// A bounded queue with explicit overflow and reusable backing storage.
pub struct EventQueue<T> {
    queue: VecDeque<T>,
    capacity: usize,
    overflow: Overflow,
    metrics: EventQueueMetrics,
}
impl<T> EventQueue<T> {
    /// Creates an empty queue. Storage grows lazily as events are accepted.
    pub fn new(capacity: usize, overflow: Overflow) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity,
            overflow,
            metrics: EventQueueMetrics::default(),
        }
    }
    /// Returns the evicted value, or the rejected input as Err. Zero capacity always rejects.
    pub fn send(&mut self, value: T) -> Result<Option<T>, T> {
        let evicted = if self.is_full() {
            if self.capacity == 0 || self.overflow == Overflow::RejectNewest {
                self.metrics.rejected = self.metrics.rejected.saturating_add(1);
                return Err(value);
            }
            self.metrics.evicted = self.metrics.evicted.saturating_add(1);
            self.queue.pop_front()
        } else {
            None
        };
        self.queue.push_back(value);
        self.metrics.accepted = self.metrics.accepted.saturating_add(1);
        self.metrics.high_water_mark = self.metrics.high_water_mark.max(self.queue.len());
        Ok(evicted)
    }
    /// Moves every event in `batch` to the queue's tail in vector order, or none.
    ///
    /// Returns the accepted event count and empties `batch` on success, retaining
    /// its allocation for reuse. An empty batch succeeds with `Ok(0)`, including
    /// on a full or zero-capacity queue, and does not change metrics.
    ///
    /// If `batch.len()` exceeds [`Self::remaining_capacity`], returns the requested
    /// and available counts in [`EventBatchError`]. The queue and `batch` retain
    /// all their events in their original order, without allocating or dropping
    /// events. Rejection increases `metrics().rejected` by the entire batch size;
    /// success increases `metrics().accepted` by that size and updates the high
    /// water mark. Counters saturate. Batches never evict, even under
    /// [`Overflow::EvictOldest`]; [`Self::send`] keeps its configured policy.
    ///
    /// No payload is cloned and no temporary event storage is allocated. Queue
    /// storage may grow on success, with space reserved before any events move.
    /// As with [`Self::send`], allocation failure is not a capacity-error result.
    ///
    /// ```
    /// use ferravex_engine::events::{EventBatchError, EventQueue, Overflow};
    ///
    /// let mut events = EventQueue::new(2, Overflow::EvictOldest);
    /// events.send(String::from("previous")).unwrap();
    /// let mut batch = vec![String::from("hit"), String::from("sound")];
    /// assert_eq!(events.try_send_batch(&mut batch), Err(EventBatchError {
    ///     requested: 2,
    ///     remaining_capacity: 1,
    /// }));
    /// assert_eq!(batch.len(), 2); // The caller still owns both events.
    /// assert_eq!(events.pop().as_deref(), Some("previous"));
    /// assert_eq!(events.try_send_batch(&mut batch), Ok(2));
    /// assert!(batch.is_empty());
    /// assert!(events.iter().map(String::as_str).eq(["hit", "sound"]));
    /// assert_eq!(events.metrics().accepted, 3);
    /// assert_eq!(events.metrics().rejected, 2);
    /// assert_eq!(events.metrics().evicted, 0);
    /// ```
    pub fn try_send_batch(&mut self, batch: &mut Vec<T>) -> Result<usize, EventBatchError> {
        let requested = batch.len();
        let remaining_capacity = self.remaining_capacity();
        let count = u64::try_from(requested).unwrap_or(u64::MAX);
        if requested > remaining_capacity {
            self.metrics.rejected = self.metrics.rejected.saturating_add(count);
            return Err(EventBatchError {
                requested,
                remaining_capacity,
            });
        }
        self.queue.reserve(requested);
        self.queue.extend(batch.drain(..));
        self.metrics.accepted = self.metrics.accepted.saturating_add(count);
        self.metrics.high_water_mark = self.metrics.high_water_mark.max(self.queue.len());
        Ok(requested)
    }
    /// Removes the oldest queued event.
    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
    /// Borrows events in send order without allocating or consuming them.
    pub fn iter(&self) -> vec_deque::Iter<'_, T> {
        self.queue.iter()
    }
    /// Removes all events in send order without allocating or cloning.
    ///
    /// Dropping the iterator discards any unyielded events. The queue keeps its
    /// backing storage for future sends. Use [`Self::drain_up_to`] to leave a tail
    /// queued for later processing.
    pub fn drain(&mut self) -> vec_deque::Drain<'_, T> {
        self.queue.drain(..)
    }
    /// Removes up to `limit` oldest events without allocating or cloning.
    ///
    /// Dropping the iterator discards unyielded events within the selected
    /// prefix; events beyond that prefix remain queued in send order. A zero
    /// limit leaves all events queued.
    pub fn drain_up_to(&mut self, limit: usize) -> vec_deque::Drain<'_, T> {
        let count = limit.min(self.queue.len());
        self.queue.drain(..count)
    }
    /// Number of queued events.
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    /// Whether no events are queued.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    /// Maximum retained event count, independent of backing storage capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Number of sends that fit before the overflow policy takes effect.
    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.queue.len()
    }
    /// Whether a send will invoke the overflow policy. True at zero capacity.
    pub fn is_full(&self) -> bool {
        self.queue.len() == self.capacity
    }
    /// Total rejected and evicted events since the last metrics reset, saturating
    /// at [`u64::MAX`]. Explicit pops, drains and clears are excluded.
    pub fn dropped(&self) -> u64 {
        self.metrics.rejected.saturating_add(self.metrics.evicted)
    }
    /// Returns a copy of the counters without changing queued events.
    pub fn metrics(&self) -> EventQueueMetrics {
        self.metrics
    }
    /// Returns the previous metrics and starts a new observation interval.
    ///
    /// Queued events remain intact. Counters become zero; the new high-water
    /// mark starts at the current queue length.
    pub fn reset_metrics(&mut self) -> EventQueueMetrics {
        std::mem::replace(
            &mut self.metrics,
            EventQueueMetrics {
                high_water_mark: self.queue.len(),
                ..EventQueueMetrics::default()
            },
        )
    }
    /// Discards queued events while retaining backing storage and metrics.
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

    #[test]
    fn each_overflow_policy_reports_backpressure() {
        for policy in [Overflow::RejectNewest, Overflow::EvictOldest] {
            let mut q = EventQueue::new(2, policy);
            assert_eq!((q.capacity(), q.remaining_capacity()), (2, 2));
            assert!(!q.is_full());
            q.send(1).unwrap();
            q.send(2).unwrap();
            assert!(q.is_full());
            assert_eq!(q.remaining_capacity(), 0);
            match policy {
                Overflow::RejectNewest => {
                    assert_eq!(q.send(3), Err(3));
                    assert!(q.iter().copied().eq([1, 2]));
                    assert_eq!(
                        q.metrics(),
                        EventQueueMetrics {
                            accepted: 2,
                            rejected: 1,
                            evicted: 0,
                            high_water_mark: 2,
                        }
                    );
                }
                Overflow::EvictOldest => {
                    assert_eq!(q.send(3), Ok(Some(1)));
                    assert!(q.iter().copied().eq([2, 3]));
                    assert_eq!(
                        q.metrics(),
                        EventQueueMetrics {
                            accepted: 3,
                            rejected: 0,
                            evicted: 1,
                            high_water_mark: 2,
                        }
                    );
                }
            }
            assert_eq!(q.dropped(), 1);
        }
    }

    #[test]
    fn zero_capacity_always_rejects_and_has_empty_drains() {
        for policy in [Overflow::RejectNewest, Overflow::EvictOldest] {
            let mut q = EventQueue::new(0, policy);
            assert!(q.is_empty());
            assert!(q.is_full());
            assert_eq!((q.capacity(), q.remaining_capacity()), (0, 0));
            assert_eq!(q.send(7), Err(7));
            assert_eq!(q.send(8), Err(8));
            assert_eq!(q.iter().len(), 0);
            assert_eq!(q.drain().next(), None);
            assert_eq!(q.drain_up_to(usize::MAX).next(), None);
            assert_eq!(
                q.metrics(),
                EventQueueMetrics {
                    rejected: 2,
                    ..EventQueueMetrics::default()
                }
            );
            q.clear();
            assert_eq!(q.dropped(), 2);
            assert_eq!(q.reset_metrics().rejected, 2);
            assert_eq!(q.metrics(), EventQueueMetrics::default());
        }
    }

    #[test]
    fn drains_preserve_fifo_after_wraparound_and_reuse_storage() {
        let mut q = EventQueue::new(3, Overflow::EvictOldest);
        for event in 1..=4 {
            q.send(event).unwrap();
        }
        assert_eq!(q.pop(), Some(2));
        q.send(5).unwrap();
        assert_eq!(q.send(6), Ok(Some(3)));
        let storage_capacity = q.queue.capacity();
        let metrics = q.metrics();
        assert!(q.iter().copied().eq([4, 5, 6]));
        assert!(q.iter().rev().copied().eq([6, 5, 4]));
        assert_eq!(q.drain_up_to(0).len(), 0);
        assert_eq!(q.len(), 3);
        {
            let mut drain = q.drain_up_to(2);
            assert_eq!(drain.len(), 2);
            assert_eq!(drain.next(), Some(4));
            // The unyielded selected event (5) is removed on drop too.
        }
        assert_eq!(q.remaining_capacity(), 2);
        assert!(q.iter().copied().eq([6]));
        assert_eq!(q.metrics(), metrics);
        assert!(q.drain_up_to(usize::MAX).eq([6]));
        assert!(q.is_empty());
        assert_eq!(q.queue.capacity(), storage_capacity);
        q.send(7).unwrap();
        q.send(8).unwrap();
        assert!(q.drain().eq([7, 8]));
        assert_eq!(q.queue.capacity(), storage_capacity);
    }

    #[test]
    fn draining_moves_owned_events_and_drops_only_selected_remainder() {
        use std::{cell::Cell, rc::Rc};
        #[derive(Debug)]
        struct Event(Rc<Cell<usize>>);
        impl Drop for Event {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut q = EventQueue::new(4, Overflow::RejectNewest);
        for _ in 0..4 {
            q.send(Event(Rc::clone(&drops))).unwrap();
        }
        let first = q.drain_up_to(2).next().unwrap();
        assert_eq!(drops.get(), 1);
        assert_eq!(q.len(), 2);
        drop(first);
        assert_eq!(drops.get(), 2);
        drop(q.drain());
        assert!(q.is_empty());
        assert_eq!(drops.get(), 4);
        assert_eq!(q.dropped(), 0);
    }

    #[test]
    fn clear_preserves_metrics_and_reset_preserves_queued_events() {
        let mut q = EventQueue::new(2, Overflow::RejectNewest);
        q.send(1).unwrap();
        q.send(2).unwrap();
        assert_eq!(q.send(3), Err(3));
        let observed = q.metrics();
        q.clear();
        assert_eq!(q.metrics(), observed);
        assert_eq!(q.remaining_capacity(), 2);
        q.send(4).unwrap();
        assert_eq!(q.reset_metrics().accepted, 3);
        assert_eq!(q.dropped(), 0);
        assert_eq!(
            q.metrics(),
            EventQueueMetrics {
                high_water_mark: 1,
                ..EventQueueMetrics::default()
            }
        );
        assert!(q.drain().eq([4]));
        assert_eq!(q.metrics().high_water_mark, 1);
        q.reset_metrics();
        assert_eq!(q.metrics(), EventQueueMetrics::default());
    }

    #[test]
    fn overflow_counters_saturate_without_changing_delivery() {
        let mut q = EventQueue::new(1, Overflow::EvictOldest);
        q.send(1).unwrap();
        q.metrics.accepted = u64::MAX;
        q.metrics.evicted = u64::MAX;
        assert_eq!(q.send(2), Ok(Some(1)));
        assert_eq!(q.metrics().accepted, u64::MAX);
        assert_eq!(q.metrics().evicted, u64::MAX);
        assert_eq!(q.dropped(), u64::MAX);
        assert_eq!(q.pop(), Some(2));
        let mut rejected = EventQueue::new(0, Overflow::RejectNewest);
        rejected.metrics.rejected = u64::MAX;
        assert_eq!(rejected.send(3), Err(3));
        assert_eq!(rejected.metrics().rejected, u64::MAX);
    }

    #[test]
    fn batch_exact_fit_preserves_fifo_after_wraparound_and_reuses_buffers() {
        for policy in [Overflow::RejectNewest, Overflow::EvictOldest] {
            let mut q = EventQueue::new(4, policy);
            for value in 0..4 {
                q.send(value).unwrap();
            }
            assert_eq!(q.pop(), Some(0));
            assert_eq!(q.pop(), Some(1));
            let queue_capacity = q.queue.capacity();
            let mut batch = Vec::with_capacity(8);
            batch.extend([4, 5]);
            let batch_capacity = batch.capacity();
            let batch_ptr = batch.as_ptr();
            assert_eq!(q.try_send_batch(&mut batch), Ok(2));
            assert!(batch.is_empty());
            assert_eq!(batch.capacity(), batch_capacity);
            assert_eq!(batch.as_ptr(), batch_ptr);
            assert_eq!(q.queue.capacity(), queue_capacity);
            assert_eq!(q.remaining_capacity(), 0);
            assert!(q.iter().copied().eq([2, 3, 4, 5]));
            assert_eq!(
                q.metrics(),
                EventQueueMetrics {
                    accepted: 6,
                    high_water_mark: 4,
                    ..EventQueueMetrics::default()
                }
            );
            q.clear();
            batch.extend([6, 7, 8]);
            assert_eq!(q.try_send_batch(&mut batch), Ok(3));
            assert!(q.drain().eq([6, 7, 8]));
            assert_eq!(q.queue.capacity(), queue_capacity);
            assert_eq!(batch.capacity(), batch_capacity);
            assert_eq!(batch.as_ptr(), batch_ptr);
        }
    }

    #[test]
    fn batch_rejection_preserves_empty_partial_and_full_queues() {
        for policy in [Overflow::RejectNewest, Overflow::EvictOldest] {
            for (queued, requested) in [(0, 4), (1, 3), (3, 1)] {
                let mut q = EventQueue::new(3, policy);
                for value in 0..queued {
                    q.send(value).unwrap();
                }
                let queue_capacity = q.queue.capacity();
                let mut batch: Vec<_> = (10..10 + requested).collect();
                let batch_capacity = batch.capacity();
                let batch_ptr = batch.as_ptr();
                assert_eq!(
                    q.try_send_batch(&mut batch),
                    Err(EventBatchError {
                        requested,
                        remaining_capacity: 3 - queued,
                    })
                );
                assert!(batch.iter().copied().eq(10..10 + requested));
                assert_eq!(batch.capacity(), batch_capacity);
                assert_eq!(batch.as_ptr(), batch_ptr);
                assert!(q.iter().copied().eq(0..queued));
                assert_eq!(q.queue.capacity(), queue_capacity);
                assert_eq!(
                    q.metrics(),
                    EventQueueMetrics {
                        accepted: queued as u64,
                        rejected: requested as u64,
                        evicted: 0,
                        high_water_mark: queued,
                    }
                );
                assert_eq!(q.dropped(), requested as u64);
            }
        }
    }

    #[test]
    fn empty_batches_succeed_without_metrics_changes_even_at_zero_capacity() {
        for policy in [Overflow::RejectNewest, Overflow::EvictOldest] {
            for capacity in [0, 2] {
                let mut q = EventQueue::new(capacity, policy);
                let mut empty = Vec::with_capacity(4);
                assert_eq!(q.try_send_batch(&mut empty), Ok(0));
                assert_eq!(q.metrics(), EventQueueMetrics::default());
                for value in 0..capacity {
                    q.send(value).unwrap();
                }
                assert_eq!(
                    q.try_send_batch(&mut vec![7, 8]),
                    Err(EventBatchError {
                        requested: 2,
                        remaining_capacity: 0,
                    })
                );
                let metrics = q.metrics();
                let storage_capacity = q.queue.capacity();
                assert_eq!(q.try_send_batch(&mut empty), Ok(0));
                assert_eq!(q.metrics(), metrics);
                assert_eq!(q.metrics().rejected, 2);
                assert_eq!(q.queue.capacity(), storage_capacity);
                assert_eq!(empty.capacity(), 4);
                assert!(q.iter().copied().eq(0..capacity));
            }
        }
    }

    #[test]
    fn rejected_batch_can_retry_without_cloning_or_dropping_owned_events() {
        use std::{cell::Cell, rc::Rc};

        // Intentionally neither Clone nor Debug: batch outcomes need no payload bounds.
        struct Event {
            id: usize,
            drops: Rc<Cell<usize>>,
        }
        impl Drop for Event {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        let event = |id| Event {
            id,
            drops: Rc::clone(&drops),
        };
        let mut q = EventQueue::new(2, Overflow::EvictOldest);
        assert!(q.send(event(0)).is_ok());
        let mut batch = vec![event(1), event(2)];
        let error = q.try_send_batch(&mut batch).unwrap_err();
        assert_eq!((error.requested, error.remaining_capacity), (2, 1));
        assert_eq!(drops.get(), 0);
        assert!(batch.iter().map(|event| event.id).eq([1, 2]));
        let previous = q.pop().unwrap();
        assert_eq!(previous.id, 0);
        assert_eq!(q.try_send_batch(&mut batch), Ok(2));
        assert!(batch.is_empty());
        assert!(q.iter().map(|event| event.id).eq([1, 2]));
        assert_eq!(drops.get(), 0);
        assert_eq!(q.metrics().accepted, 3);
        assert_eq!(q.metrics().rejected, 2);
        assert_eq!(q.metrics().high_water_mark, 2);
        assert_eq!(q.metrics().evicted, 0);
        drop(batch);
        assert_eq!(drops.get(), 0);
        drop(previous);
        drop(q);
        assert_eq!(drops.get(), 3);
    }

    #[test]
    fn individual_sends_keep_their_overflow_policy_after_atomic_batches() {
        for policy in [Overflow::RejectNewest, Overflow::EvictOldest] {
            let mut q = EventQueue::new(2, policy);
            assert_eq!(q.try_send_batch(&mut vec![1, 2]), Ok(2));
            let mut batch = vec![3];
            assert!(q.try_send_batch(&mut batch).is_err());
            assert_eq!(batch, [3]);
            match policy {
                Overflow::RejectNewest => {
                    assert_eq!(q.send(4), Err(4));
                    assert!(q.iter().copied().eq([1, 2]));
                    assert_eq!(q.metrics().accepted, 2);
                    assert_eq!(q.metrics().rejected, 2);
                    assert_eq!(q.metrics().evicted, 0);
                }
                Overflow::EvictOldest => {
                    assert_eq!(q.send(4), Ok(Some(1)));
                    assert!(q.iter().copied().eq([2, 4]));
                    assert_eq!(q.metrics().accepted, 3);
                    assert_eq!(q.metrics().rejected, 1);
                    assert_eq!(q.metrics().evicted, 1);
                }
            }
            assert_eq!(q.dropped(), 2);
            let metrics = q.metrics();
            assert_eq!(q.reset_metrics(), metrics);
            assert_eq!(q.len(), 2);
            assert_eq!(
                q.metrics(),
                EventQueueMetrics {
                    high_water_mark: 2,
                    ..EventQueueMetrics::default()
                }
            );
            q.clear();
            assert_eq!(q.try_send_batch(&mut batch), Ok(1));
            assert_eq!(q.metrics().accepted, 1);
            assert_eq!(q.metrics().high_water_mark, 2);
            assert_eq!(q.dropped(), 0);
        }
    }

    #[test]
    fn batch_counters_saturate_without_affecting_delivery_or_evictions() {
        let mut q = EventQueue::new(3, Overflow::EvictOldest);
        q.send(0).unwrap();
        q.metrics.accepted = u64::MAX - 1;
        q.metrics.rejected = u64::MAX - 1;
        q.metrics.evicted = u64::MAX - 1;
        assert_eq!(q.try_send_batch(&mut vec![1, 2]), Ok(2));
        assert_eq!(q.metrics().accepted, u64::MAX);
        assert!(q.try_send_batch(&mut vec![3, 4]).is_err());
        assert_eq!(q.metrics().rejected, u64::MAX);
        assert_eq!(q.metrics().evicted, u64::MAX - 1);
        assert_eq!(q.dropped(), u64::MAX);
        assert!(q.iter().copied().eq([0, 1, 2]));
        assert_eq!(q.send(5), Ok(Some(0)));
        assert_eq!(q.metrics().evicted, u64::MAX);
        assert!(q.drain().eq([1, 2, 5]));
    }

    #[test]
    fn zero_sized_batches_use_event_counts_with_maximum_logical_capacity() {
        let mut q = EventQueue::new(usize::MAX, Overflow::RejectNewest);
        assert_eq!(q.try_send_batch(&mut vec![(); 3]), Ok(3));
        assert_eq!(q.len(), 3);
        assert_eq!(q.remaining_capacity(), usize::MAX - 3);
        assert_eq!(q.metrics().accepted, 3);
        assert_eq!(q.metrics().high_water_mark, 3);
        assert_eq!(q.drain().count(), 3);
    }
}
