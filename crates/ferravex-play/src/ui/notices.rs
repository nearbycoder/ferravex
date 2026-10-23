//! Bounded transient text/semantic notices, driven by accepted game events.
//!
//! Storage is inline, with no allocation by the queue. Payloads and keys may
//! borrow caller data or use fixed-size types; their own allocations and work
//! are the caller's responsibility. The queue neither validates game actions
//! nor deduplicates event IDs: deliver each accepted event once, after rules
//! run (and after any network/event deduplication). A key groups notices, not
//! events. Keep must-read outcomes/result panels outside this transient queue.
//!
//! All live entries age, including those hidden by higher priorities. The host
//! chooses simulation or presentation seconds in [`NoticeQueue::advance`]; no
//! clock, renderer, motion/flash preference or gameplay state is consulted.
//! Render the same text under reduced motion, suppressing only host animation.
//!
//! This Pinball-style recipe consumes an accepted-score stream, never a score
//! difference or an attempted bumper hit. Its saturating sum is explicit caller
//! policy; a rejected request produces no accepted-score event. A real Pinball
//! adapter can feed `PinballFeedback::points` after each `advance` exactly once.
//!
//! ```
//! use ferravex_play::ui::notices::{
//!     Notice, NoticeChange, NoticeLifetime, NoticePriority, NoticeQueue,
//! };
//!
//! #[derive(Debug, PartialEq)]
//! struct RecoveredPoints(u32);
//! enum RuleResult { AcceptedScore(u32), RejectedRequest }
//! let events = [RuleResult::AcceptedScore(100), RuleResult::RejectedRequest,
//!               RuleResult::AcceptedScore(200)];
//! let mut notices = NoticeQueue::<4, RecoveredPoints>::new();
//! for event in events {
//!     if let RuleResult::AcceptedScore(points) = event {
//!         let change = notices.coalesce_with(Notice {
//!             key: Some(1), priority: NoticePriority::Normal,
//!             message: RecoveredPoints(points), lifetime_seconds: 2.,
//!         }, NoticeLifetime::Refresh, |a, b| {
//!             RecoveredPoints(a.0.saturating_add(b.0))
//!         }).unwrap();
//!         // Every displaced/incoming payload is returned for caller disposal.
//!         assert!(matches!(change, NoticeChange::Inserted { evicted: None }
//!             | NoticeChange::Coalesced { .. }));
//!     }
//! }
//! assert_eq!(notices.front().unwrap().notice.message, RecoveredPoints(300));
//! assert_eq!(notices.advance(0.5), Ok(0));
//! assert_eq!(notices.front().unwrap().remaining_seconds, 1.5);
//! let dismissed = notices.dismiss(&1).unwrap();
//! assert_eq!(dismissed.message.0, 300);
//! notices.clear(); // Restart transient presentation; no game state is changed.
//! assert!(notices.is_empty());
//! ```

/// Higher priorities appear first and can displace strictly lower priorities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoticePriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Caller-owned notice specification. `None` keys are always independent.
///
/// Insertion validates a finite, strictly positive lifetime in seconds. Invalid
/// specifications are returned intact. This duration remains the authored
/// specification; read [`NoticeView::remaining_seconds`] for current expiry.
#[derive(Debug, PartialEq)]
pub struct Notice<T, K = u64> {
    pub key: Option<K>,
    pub priority: NoticePriority,
    pub message: T,
    pub lifetime_seconds: f64,
}

/// Policy for an existing matching key. This is grouping, not event deduplication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeDuplicate {
    /// Reject the incoming notice, preserving the existing entry and expiry.
    Keep,
    /// Replace even a higher priority matching notice; restart its lifetime.
    Replace,
}

/// Expiry policy when a matching key is coalesced successfully.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeLifetime {
    KeepRemaining,
    /// Use the incoming duration, which can shorten the remaining lifetime.
    Refresh,
    /// Add the incoming duration, saturating at finite `f64::MAX` on overflow.
    Extend,
}

/// Rejection leaves queue contents and expiry unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeRejection {
    InvalidLifetime,
    DuplicateKey,
    /// Zero capacity or no entry with strictly lower priority.
    Capacity,
}

/// A rejected insertion retains ownership of its entire input.
#[derive(Debug, PartialEq)]
pub struct NoticeRejected<T, K = u64> {
    pub reason: NoticeRejection,
    pub notice: Notice<T, K>,
}

/// Successful submission, returning payloads the queue no longer needs.
/// No caller payload/key destructor is invoked by successful submission itself.
#[derive(Debug, PartialEq)]
pub enum NoticeChange<T, K = u64> {
    Inserted {
        evicted: Option<Notice<T, K>>,
    },
    Replaced {
        previous: Notice<T, K>,
    },
    Coalesced {
        previous_message: T,
        incoming: Notice<T, K>,
    },
}

/// A presentation snapshot borrowing the queue; iteration never consumes text.
#[derive(Debug)]
pub struct NoticeView<'a, T, K = u64> {
    pub notice: &'a Notice<T, K>,
    pub remaining_seconds: f64,
}

/// Invalid delta: nonfinite or negative. No time or entries were changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoticeTimeError;

#[derive(Debug)]
struct Entry<T, K> {
    notice: Notice<T, K>,
    remaining_seconds: f64,
}

/// Fixed inline storage with stable descending-priority presentation order.
///
/// Equal priorities retain arrival order. At saturation, an incoming notice can
/// evict only a strictly lower priority: the oldest entry at the lowest priority
/// is evicted first. Existing critical entries survive unrelated bursts, even
/// critical bursts. Keyed replacement explicitly bypasses that rule and counts
/// as a new arrival. Coalescing keeps the original key/specification, promotes
/// priority to the larger value, and keeps its place unless promoted; promotion
/// joins behind existing entries of the new priority.
///
/// Submission/removal use O(N) queue work and O(1) additional storage, excluding
/// caller equality, merge and destructor work. Expiring multiple entries can
/// take O(N²) work because each removal preserves order before disposal. Clear
/// is O(N). No `Clone`, `Default` or `Copy` bound is required for messages or keys.
/// Choose N/payload sizes to suit stack limits. Timing uses ordinary f64
/// arithmetic: deltas below the representable precision may not reduce a large
/// remaining lifetime; no cross-platform bitwise determinism is promised.
///
/// # Panic and disposal boundaries
///
/// Equality and merge callbacks run before structural mutation; a panic leaves
/// queue metadata unchanged. Interior mutation/side effects in caller code are
/// not rolled back. Coalescing receives shared references, returns a new payload
/// and returns both discarded inputs to the host. `advance`, `clear`, dropping
/// the queue, and dropping returned values can execute caller destructors.
/// A destructor panic is not transactional: earlier changes remain, later work
/// may be incomplete. Removed slots are detached/compacted before disposal, so
/// storage/length remain valid. Destructors must not panic for normal use.
#[derive(Debug)]
pub struct NoticeQueue<const N: usize, T, K = u64> {
    entries: [Option<Entry<T, K>>; N],
    len: usize,
}

impl<const N: usize, T, K> Default for NoticeQueue<N, T, K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, T, K> NoticeQueue<N, T, K> {
    /// Empty, reusable storage. Zero capacity is supported and rejects all input.
    pub fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            len: 0,
        }
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Highest priority, oldest entry at that priority; no visibility side effect.
    pub fn front(&self) -> Option<NoticeView<'_, T, K>> {
        self.iter().next()
    }

    /// All entries in presentation order, including ones a host chooses to hide.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = NoticeView<'_, T, K>> {
        self.entries[..self.len].iter().map(|entry| {
            let entry = entry.as_ref().expect("occupied prefix");
            NoticeView {
                notice: &entry.notice,
                remaining_seconds: entry.remaining_seconds,
            }
        })
    }

    /// Remove the front notice without dropping its payload, including unkeyed text.
    pub fn pop_front(&mut self) -> Option<Notice<T, K>> {
        (!self.is_empty()).then(|| self.remove(0).notice)
    }

    /// Age every entry by explicit finite, nonnegative seconds. Zero is a no-op;
    /// a delta reaching/exceeding remaining lifetime expires and drops the entry.
    /// Subtraction clamps at zero without overflow. Returns the number expired.
    /// Hidden entries expire too; no total wall time is accumulated.
    pub fn advance(&mut self, delta_seconds: f64) -> Result<usize, NoticeTimeError> {
        if !delta_seconds.is_finite() || delta_seconds < 0. {
            return Err(NoticeTimeError);
        }
        let mut expired = 0;
        let mut index = 0;
        while index < self.len {
            let entry = self.entries[index].as_mut().expect("occupied prefix");
            entry.remaining_seconds = (entry.remaining_seconds - delta_seconds).max(0.);
            if entry.remaining_seconds == 0. {
                let removed = self.remove(index);
                expired += 1;
                drop(removed);
            } else {
                index += 1;
            }
        }
        Ok(expired)
    }

    /// Drop all transient entries on restart, reusing the same inline storage.
    /// No key or remaining lifetime survives reset. The queue owns no accumulated
    /// clock, arrival counter or external event history.
    pub fn clear(&mut self) {
        while self.len > 0 {
            drop(self.remove(self.len - 1));
        }
    }

    fn remove(&mut self, index: usize) -> Entry<T, K> {
        let entry = self.entries[index].take().expect("occupied prefix");
        for next in index + 1..self.len {
            self.entries[next - 1] = self.entries[next].take();
        }
        self.len -= 1;
        entry
    }

    fn insert_sorted(&mut self, entry: Entry<T, K>) {
        let index = self.entries[..self.len]
            .iter()
            .position(|other| other.as_ref().unwrap().notice.priority < entry.notice.priority)
            .unwrap_or(self.len);
        for previous in (index..self.len).rev() {
            self.entries[previous + 1] = self.entries[previous].take();
        }
        self.entries[index] = Some(entry);
        self.len += 1;
    }
}

impl<const N: usize, T, K: Eq> NoticeQueue<N, T, K> {
    /// Insert or apply an explicit duplicate-key policy. All lifetimes are
    /// validated before key matching, including when keeping an existing entry.
    ///
    /// ```
    /// use ferravex_play::ui::notices::{
    ///     Notice, NoticeDuplicate, NoticePriority, NoticeQueue,
    /// };
    /// let mut notices = NoticeQueue::<2, &str, &str>::new();
    /// notices.insert(Notice {
    ///     key: Some("objective"), priority: NoticePriority::High,
    ///     message: "Altar I awakened. The second altar is ready.",
    ///     lifetime_seconds: 3.5,
    /// }, NoticeDuplicate::Replace).unwrap(); // After objective acceptance.
    /// assert_eq!(notices.capacity(), 2);
    /// assert_eq!(notices.iter().len(), 1);
    /// assert_eq!(notices.pop_front().unwrap().key, Some("objective"));
    /// assert!(notices.is_empty());
    /// ```
    pub fn insert(
        &mut self,
        notice: Notice<T, K>,
        duplicate: NoticeDuplicate,
    ) -> Result<NoticeChange<T, K>, NoticeRejected<T, K>> {
        self.validate(notice).and_then(|notice| {
            if let Some(index) = self.matching(&notice.key) {
                match duplicate {
                    NoticeDuplicate::Keep => Err(NoticeRejected {
                        reason: NoticeRejection::DuplicateKey,
                        notice,
                    }),
                    NoticeDuplicate::Replace => {
                        let previous = self.remove(index).notice;
                        self.insert_sorted(Entry {
                            remaining_seconds: notice.lifetime_seconds,
                            notice,
                        });
                        Ok(NoticeChange::Replaced { previous })
                    }
                }
            } else {
                self.insert_new(notice)
            }
        })
    }

    /// Merge matching payloads using caller arithmetic/overflow policy. Missing
    /// keys follow ordinary insertion/saturation, without invoking `merge`.
    /// Invalid lifetimes also skip the callback. No payload is cloned or dropped
    /// internally. Key equality/merge panics occur before queue mutation.
    pub fn coalesce_with(
        &mut self,
        notice: Notice<T, K>,
        lifetime: NoticeLifetime,
        merge: impl FnOnce(&T, &T) -> T,
    ) -> Result<NoticeChange<T, K>, NoticeRejected<T, K>> {
        let notice = self.validate(notice)?;
        let Some(index) = self.matching(&notice.key) else {
            return self.insert_new(notice);
        };
        let entry = self.entries[index].as_ref().expect("occupied prefix");
        let remaining_seconds = match lifetime {
            NoticeLifetime::KeepRemaining => entry.remaining_seconds,
            NoticeLifetime::Refresh => notice.lifetime_seconds,
            NoticeLifetime::Extend => {
                (entry.remaining_seconds + notice.lifetime_seconds).min(f64::MAX)
            }
        };
        let message = merge(&entry.notice.message, &notice.message);
        let entry = self.entries[index].as_mut().expect("occupied prefix");
        let previous_message = std::mem::replace(&mut entry.notice.message, message);
        entry.remaining_seconds = remaining_seconds;
        if notice.priority > entry.notice.priority {
            entry.notice.priority = notice.priority;
            let entry = self.remove(index);
            self.insert_sorted(entry);
        }
        Ok(NoticeChange::Coalesced {
            previous_message,
            incoming: notice,
        })
    }

    /// Remove a keyed notice, returning ownership. A stale/missing key is a no-op.
    pub fn dismiss(&mut self, key: &K) -> Option<Notice<T, K>> {
        let index = self.entries[..self.len]
            .iter()
            .position(|entry| entry.as_ref().unwrap().notice.key.as_ref() == Some(key))?;
        Some(self.remove(index).notice)
    }

    fn matching(&self, key: &Option<K>) -> Option<usize> {
        key.as_ref().and_then(|key| {
            self.entries[..self.len]
                .iter()
                .position(|entry| entry.as_ref().unwrap().notice.key.as_ref() == Some(key))
        })
    }

    fn validate(&self, notice: Notice<T, K>) -> Result<Notice<T, K>, NoticeRejected<T, K>> {
        if !notice.lifetime_seconds.is_finite() || notice.lifetime_seconds <= 0. {
            Err(NoticeRejected {
                reason: NoticeRejection::InvalidLifetime,
                notice,
            })
        } else {
            Ok(notice)
        }
    }

    fn insert_new(
        &mut self,
        notice: Notice<T, K>,
    ) -> Result<NoticeChange<T, K>, NoticeRejected<T, K>> {
        let evicted = if self.len == N {
            let candidate = self.entries[..self.len].last().and_then(|last| {
                let lowest = last.as_ref().unwrap().notice.priority;
                (lowest < notice.priority).then(|| {
                    self.entries[..self.len]
                        .iter()
                        .position(|entry| entry.as_ref().unwrap().notice.priority == lowest)
                        .unwrap()
                })
            });
            let Some(index) = candidate else {
                return Err(NoticeRejected {
                    reason: NoticeRejection::Capacity,
                    notice,
                });
            };
            Some(self.remove(index).notice)
        } else {
            None
        };
        self.insert_sorted(Entry {
            remaining_seconds: notice.lifetime_seconds,
            notice,
        });
        Ok(NoticeChange::Inserted { evicted })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, panic::AssertUnwindSafe, rc::Rc};

    fn notice(key: u64, priority: NoticePriority, message: u32) -> Notice<u32> {
        Notice {
            key: Some(key),
            priority,
            message,
            lifetime_seconds: 2.,
        }
    }

    fn keys<const N: usize>(queue: &NoticeQueue<N, u32>) -> Vec<u64> {
        queue.iter().map(|view| view.notice.key.unwrap()).collect()
    }

    #[test]
    fn priority_saturation_preserves_critical_and_evicts_oldest_lowest() {
        use NoticePriority::*;
        let mut queue = NoticeQueue::<4, u32>::new();
        for (key, priority) in [(1, Low), (2, Critical), (3, Low), (4, Normal)] {
            queue
                .insert(notice(key, priority, 100), NoticeDuplicate::Keep)
                .unwrap();
        }
        assert_eq!(keys(&queue), [2, 4, 1, 3]);
        let change = queue
            .insert(notice(5, High, 100), NoticeDuplicate::Keep)
            .unwrap();
        assert!(matches!(
            change,
            NoticeChange::Inserted {
                evicted: Some(Notice { key: Some(1), .. })
            }
        ));
        assert_eq!(keys(&queue), [2, 5, 4, 3]);
        for key in 6..10 {
            queue
                .insert(notice(key, Critical, 100), NoticeDuplicate::Keep)
                .ok();
        }
        assert_eq!(keys(&queue), [2, 6, 7, 8]);
        let rejected = queue
            .insert(notice(10, Critical, 100), NoticeDuplicate::Keep)
            .unwrap_err();
        assert_eq!(rejected.reason, NoticeRejection::Capacity);
        assert_eq!(rejected.notice.key, Some(10));
        assert_eq!(keys(&queue), [2, 6, 7, 8]);
    }

    #[test]
    fn replacement_and_keep_are_explicit_at_capacity() {
        use NoticePriority::*;
        let mut queue = NoticeQueue::<2, u32>::new();
        queue
            .insert(notice(1, Critical, 10), NoticeDuplicate::Keep)
            .unwrap();
        queue
            .insert(notice(2, Normal, 20), NoticeDuplicate::Keep)
            .unwrap();
        queue.advance(1.).unwrap();
        let rejected = queue
            .insert(notice(1, Low, 99), NoticeDuplicate::Keep)
            .unwrap_err();
        assert_eq!(rejected.reason, NoticeRejection::DuplicateKey);
        assert_eq!(queue.front().unwrap().remaining_seconds, 1.);
        assert_eq!(queue.front().unwrap().notice.message, 10);
        assert!(matches!(
            queue
                .insert(notice(1, Normal, 99), NoticeDuplicate::Replace)
                .unwrap(),
            NoticeChange::Replaced {
                previous: Notice { message: 10, .. }
            }
        ));
        assert_eq!(keys(&queue), [2, 1]); // Replacement is a new arrival.
        assert_eq!(queue.iter().nth(1).unwrap().remaining_seconds, 2.);
        assert_eq!(queue.dismiss(&1).unwrap().message, 99);
        assert_eq!(queue.dismiss(&1), None);
        assert_eq!(queue.pop_front().unwrap().key, Some(2));
        assert_eq!(queue.pop_front(), None);
    }

    #[test]
    fn coalescing_retains_order_promotes_priority_and_returns_inputs() {
        use NoticePriority::*;
        let mut queue = NoticeQueue::<3, u32>::new();
        for (key, priority) in [(1, High), (2, Normal), (3, Normal)] {
            queue
                .insert(notice(key, priority, 100), NoticeDuplicate::Keep)
                .unwrap();
        }
        queue.advance(0.5).unwrap();
        let result = queue
            .coalesce_with(notice(2, Low, 50), NoticeLifetime::KeepRemaining, |a, b| {
                a.saturating_add(*b)
            })
            .unwrap();
        assert!(matches!(
            result,
            NoticeChange::Coalesced {
                previous_message: 100,
                incoming: Notice { message: 50, .. }
            }
        ));
        assert_eq!(keys(&queue), [1, 2, 3]);
        assert_eq!(queue.iter().nth(1).unwrap().remaining_seconds, 1.5);
        queue
            .coalesce_with(notice(3, High, 25), NoticeLifetime::Refresh, |a, b| a + b)
            .unwrap();
        assert_eq!(keys(&queue), [1, 3, 2]);
        let promoted = queue.iter().nth(1).unwrap();
        assert_eq!(promoted.notice.message, 125);
        assert_eq!(promoted.remaining_seconds, 2.);
    }

    #[test]
    fn coalescing_lifetimes_and_point_overflow_saturate_without_infinity() {
        let mut queue = NoticeQueue::<1, u32>::new();
        queue
            .insert(
                notice(1, NoticePriority::Normal, u32::MAX),
                NoticeDuplicate::Keep,
            )
            .unwrap();
        queue.advance(1.5).unwrap();
        let mut incoming = notice(1, NoticePriority::Low, 100);
        incoming.lifetime_seconds = 0.25;
        queue
            .coalesce_with(incoming, NoticeLifetime::Refresh, |a, b| {
                a.saturating_add(*b)
            })
            .unwrap();
        assert_eq!(queue.front().unwrap().remaining_seconds, 0.25);
        assert_eq!(queue.front().unwrap().notice.message, u32::MAX);
        for _ in 0..2 {
            let mut incoming = notice(1, NoticePriority::Low, 0);
            incoming.lifetime_seconds = f64::MAX;
            queue
                .coalesce_with(incoming, NoticeLifetime::Extend, |a, _| *a)
                .unwrap();
        }
        assert_eq!(queue.front().unwrap().remaining_seconds, f64::MAX);
        assert_eq!(queue.advance(f64::MAX), Ok(1));
    }

    #[test]
    fn invalid_time_is_atomic_and_skips_mergers_even_on_duplicate_or_empty_queue() {
        let mut queue = NoticeQueue::<2, u32>::new();
        queue
            .insert(notice(1, NoticePriority::Normal, 5), NoticeDuplicate::Keep)
            .unwrap();
        for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 0., -0.] {
            let mut invalid = notice(1, NoticePriority::Critical, 999);
            invalid.lifetime_seconds = seconds;
            let rejected = queue
                .coalesce_with(invalid, NoticeLifetime::Extend, |_, _| {
                    panic!("invalid input must not merge")
                })
                .unwrap_err();
            assert_eq!(rejected.reason, NoticeRejection::InvalidLifetime);
            assert_eq!(rejected.notice.message, 999);
            let mut invalid = notice(1, NoticePriority::Low, 0);
            invalid.lifetime_seconds = seconds;
            assert_eq!(
                queue
                    .insert(invalid, NoticeDuplicate::Keep)
                    .unwrap_err()
                    .reason,
                NoticeRejection::InvalidLifetime
            );
        }
        for delta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
            assert_eq!(queue.advance(delta), Err(NoticeTimeError));
            assert_eq!(queue.front().unwrap().remaining_seconds, 2.);
            assert_eq!(queue.front().unwrap().notice.message, 5);
        }
        assert_eq!(queue.advance(-0.), Ok(0));
        let mut empty = NoticeQueue::<0, u32>::new();
        assert_eq!(empty.advance(f64::NAN), Err(NoticeTimeError));
    }

    #[test]
    fn zero_capacity_unkeyed_and_missing_key_do_not_invoke_merge() {
        let mut zero = NoticeQueue::<0, u32>::new();
        assert_eq!(zero.capacity(), 0);
        assert_eq!(
            zero.coalesce_with(
                notice(1, NoticePriority::Critical, 5),
                NoticeLifetime::Refresh,
                |_, _| panic!("no match")
            )
            .unwrap_err()
            .reason,
            NoticeRejection::Capacity
        );
        assert!(zero.front().is_none());
        assert_eq!(zero.iter().len(), 0);
        assert_eq!(zero.advance(1.), Ok(0));
        assert!(zero.dismiss(&1).is_none());
        zero.clear();
        let mut queue = NoticeQueue::<2, u32>::new();
        for _ in 0..2 {
            let mut item = notice(1, NoticePriority::Normal, 5);
            item.key = None;
            queue
                .coalesce_with(item, NoticeLifetime::Refresh, |_, _| panic!("unkeyed"))
                .unwrap();
        }
        assert_eq!(queue.len(), 2);
        assert!(queue.dismiss(&1).is_none());
    }

    #[test]
    fn expiry_ages_hidden_entries_exactly_and_reset_reuses_storage() {
        let mut queue = NoticeQueue::<4, u32>::default();
        let storage = queue.entries.as_ptr();
        for round in 0..8 {
            for key in 1..=4 {
                let priority = if key == 4 {
                    NoticePriority::Critical
                } else {
                    NoticePriority::Low
                };
                let mut item = notice(key, priority, round);
                item.lifetime_seconds = key as f64;
                queue.insert(item, NoticeDuplicate::Keep).unwrap();
            }
            assert_eq!(queue.advance(0.), Ok(0));
            assert_eq!(queue.advance(1.), Ok(1));
            assert_eq!(keys(&queue), [4, 2, 3]);
            assert_eq!(queue.advance(1.), Ok(1));
            assert_eq!(keys(&queue), [4, 3]);
            queue.clear();
            assert_eq!(queue.len(), 0);
            assert!(queue.entries.iter().all(Option::is_none));
            assert_eq!(queue.entries.as_ptr(), storage);
            assert_eq!(queue.advance(f64::MAX), Ok(0));
        }
    }

    #[test]
    fn accepted_rule_results_coalesce_once_and_preferences_do_not_remove_text() {
        // A tiny authoritative claim rule: rejected repeats emit nothing.
        let mut claimed = [false; 2];
        let mut queue = NoticeQueue::<2, u32>::new();
        for (id, points) in [(0, 100), (0, 999), (1, 200)] {
            let accepted_points = if claimed[id] {
                None
            } else {
                claimed[id] = true;
                Some(points)
            };
            if let Some(points) = accepted_points {
                queue
                    .coalesce_with(
                        notice(1, NoticePriority::Normal, points),
                        NoticeLifetime::Refresh,
                        |a, b| a.saturating_add(*b),
                    )
                    .unwrap();
            }
        }
        assert_eq!(queue.front().unwrap().notice.message, 300);
        for motion in [false, true] {
            let policy = crate::feedback::FeedbackPolicy::new(motion, true, 0.).unwrap();
            assert_eq!(policy.reduced_motion(), motion);
            // Paint semantic text for either policy; only animation consults it.
            assert_eq!(queue.front().unwrap().notice.message, 300);
        }
    }

    #[test]
    fn merge_panic_preserves_queue_and_new_key_never_calls_merge() {
        let mut queue = NoticeQueue::<1, u32>::new();
        queue
            .coalesce_with(
                notice(1, NoticePriority::Normal, 5),
                NoticeLifetime::Refresh,
                |_, _| panic!("no match"),
            )
            .unwrap();
        queue.advance(1.).unwrap();
        let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
            queue.coalesce_with(
                notice(1, NoticePriority::Critical, 10),
                NoticeLifetime::Extend,
                |_, _| panic!("caller merge"),
            )
        }));
        assert!(panic.is_err());
        let retained = queue.front().unwrap();
        assert_eq!(retained.notice.message, 5);
        assert_eq!(retained.notice.priority, NoticePriority::Normal);
        assert_eq!(retained.remaining_seconds, 1.);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn equality_panic_preserves_queue_and_expiry() {
        #[derive(Debug)]
        struct Key(bool);
        impl PartialEq for Key {
            fn eq(&self, other: &Self) -> bool {
                assert!(!other.0, "caller equality");
                true
            }
        }
        impl Eq for Key {}
        let mut queue = NoticeQueue::<1, u32, Key>::new();
        let make = |panic| Notice {
            key: Some(Key(panic)),
            message: 5,
            priority: NoticePriority::Normal,
            lifetime_seconds: 2.,
        };
        queue.insert(make(false), NoticeDuplicate::Keep).unwrap();
        queue.advance(1.).unwrap();
        let panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
            queue.insert(make(true), NoticeDuplicate::Replace)
        }));
        assert!(panic.is_err());
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.front().unwrap().remaining_seconds, 1.);
        assert!(!queue.front().unwrap().notice.key.as_ref().unwrap().0);
    }

    #[test]
    fn borrowed_nonclone_payloads_and_returned_drops_stay_caller_controlled() {
        #[derive(Debug)]
        struct Message<'a>(&'a Cell<u32>);
        impl Drop for Message<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Cell::new(0);
        let make = |priority| Notice {
            key: Some("caption"),
            priority,
            message: Message(&drops),
            lifetime_seconds: 1.,
        };
        let mut queue = NoticeQueue::<1, Message<'_>, &str>::new();
        queue
            .insert(make(NoticePriority::Low), NoticeDuplicate::Keep)
            .unwrap();
        let change = queue
            .coalesce_with(
                make(NoticePriority::High),
                NoticeLifetime::Refresh,
                |a, _| Message(a.0),
            )
            .unwrap();
        assert_eq!(drops.get(), 0);
        drop(change);
        assert_eq!(drops.get(), 2); // Prior and incoming messages returned.
        let replaced = queue
            .insert(make(NoticePriority::Normal), NoticeDuplicate::Replace)
            .unwrap();
        assert_eq!(drops.get(), 2);
        drop(replaced);
        assert_eq!(drops.get(), 3);
        let rejected = queue
            .insert(make(NoticePriority::Low), NoticeDuplicate::Keep)
            .unwrap_err();
        assert_eq!(drops.get(), 3);
        drop(rejected);
        assert_eq!(drops.get(), 4);
        queue.advance(1.).unwrap();
        assert_eq!(drops.get(), 5);
        queue
            .insert(make(NoticePriority::Normal), NoticeDuplicate::Keep)
            .unwrap();
        queue.clear();
        assert_eq!(drops.get(), 6);
    }

    #[test]
    fn destructor_panic_detaches_slot_before_unwind_and_storage_can_be_reused() {
        #[derive(Debug)]
        struct Message(Rc<Cell<bool>>);
        impl Drop for Message {
            fn drop(&mut self) {
                assert!(!self.0.replace(false), "caller drop");
            }
        }
        let panic_next_drop = Rc::new(Cell::new(true));
        let mut queue = NoticeQueue::<3, Message>::new();
        let make = |key, panic_on_drop| Notice {
            key: Some(key),
            priority: NoticePriority::Normal,
            message: Message(panic_on_drop),
            lifetime_seconds: 1.,
        };
        queue
            .insert(make(1, panic_next_drop.clone()), NoticeDuplicate::Keep)
            .unwrap();
        queue
            .insert(make(2, Rc::new(Cell::new(false))), NoticeDuplicate::Keep)
            .unwrap();
        queue
            .insert(make(3, Rc::new(Cell::new(false))), NoticeDuplicate::Keep)
            .unwrap();
        assert!(std::panic::catch_unwind(AssertUnwindSafe(|| queue.advance(1.))).is_err());
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.front().unwrap().notice.key, Some(2));
        assert_eq!(queue.front().unwrap().remaining_seconds, 1.); // Tail was not aged.
        assert!(queue.entries[2].is_none());
        queue
            .insert(make(4, panic_next_drop), NoticeDuplicate::Keep)
            .unwrap();
        queue.clear();
        assert!(queue.is_empty());
    }
}
