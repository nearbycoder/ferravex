//! Explicit event visibility boundaries for ordered engine systems.
//! Sends enter a pending buffer; advance publishes it and expires the previous frame.
//! Each buffer holds at most `capacity` events (total retained count <= 2*capacity).
//!
//! [`FrameEvents::read`](crate::frame_events::FrameEvents::read) and [`FrameEvents::iter`](crate::frame_events::FrameEvents::iter) let multiple listeners borrow
//! the same visible events. Drains instead transfer ownership to one consumer;
//! they never consume pending events and allocate no storage.
//! Independent [`FrameEventReader`] cursors additionally distinguish an empty
//! read from unread events lost to advance, drain or clear. Readers do not keep
//! old frames alive or change the two-buffer retention bound.
//!
//! ```
//! use ferravex_engine::frame_events::FrameEvents;
//!
//! let mut events = FrameEvents::new(2);
//! events.send(String::from("hit")).unwrap();
//! assert!(events.read().is_empty());
//! events.advance();
//! events.send(String::from("next frame")).unwrap();
//! assert_eq!(events.iter().next().map(String::as_str), Some("hit"));
//! for event in events.drain() {
//!     assert_eq!(event, "hit");
//! }
//! assert_eq!(events.pending_len(), 1);
//! events.advance();
//! assert_eq!(events.read()[0], "next frame");
//! assert_eq!(events.metrics().expired, 0); // The previous frame was drained.
//! let interval = events.reset_metrics();
//! assert_eq!(interval.accepted, 2);
//! assert_eq!(events.read()[0], "next frame"); // Reset preserves events.
//! ```
use std::{
    fmt, slice,
    sync::{Arc, OnceLock},
    vec,
};

/// An independent cursor created by [`FrameEvents::reader`].
///
/// Cloning forks the cursor at its current position; neither cursor retains
/// events. A cursor belongs to its original buffer even if that buffer moves.
/// Using it with another buffer returns [`FrameReaderError::WrongBuffer`].
#[derive(Clone, Debug)]
pub struct FrameEventReader {
    source: Arc<()>,
    frame: u64,
    next_event: u64,
    unread_frame: bool,
}

/// A cursor's status before a read or reset acknowledges any loss.
///
/// Counts describe progress since creation or the last cursor read/reset, not
/// queue metrics. In particular, [`FrameEvents::reset_metrics`] has no effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameReaderStatus {
    /// Advances since the cursor last read or reset (or was created).
    pub frames_behind: u64,
    /// Frames that expired before the cursor finished reading them.
    ///
    /// Includes wholly unobserved empty frames, but excludes the current frame
    /// and a previously fully read frame. Removal within the current frame is
    /// reported by `missed_events`, without incrementing this count.
    pub missed_frames: u64,
    /// Unread published events no longer available after advance, drain or clear.
    /// Rejected sends and cleared pending events were never published and do not count.
    pub missed_events: u64,
    /// Unread events still available in the current visible frame.
    pub unread_events: usize,
}

/// Borrowed events in send order, accompanied by the status before the read.
///
/// `status.unread_events` includes any tail left by a limited read. The returned
/// slice has already been acknowledged by the cursor, even if it is not iterated.
#[derive(Debug)]
pub struct FrameEventRead<'a, T> {
    /// The selected unread events; no events are removed from the buffer.
    pub events: &'a [T],
    /// Loss and availability before advancing the cursor.
    pub status: FrameReaderStatus,
}

/// A safe reset destination within a reader's original buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameReaderPosition {
    /// Read all events still visible, including any already read by this cursor.
    Oldest,
    /// Skip all currently visible events; future published events remain unread.
    Latest,
}

/// A cursor was supplied to a buffer other than the one that created it.
///
/// ```
/// use ferravex_engine::frame_events::{FrameEvents, FrameReaderError};
/// let first = FrameEvents::<u8>::new(1);
/// let second = FrameEvents::<u8>::new(1);
/// let mut reader = first.reader();
/// assert_eq!(second.read_with(&mut reader).unwrap_err(), FrameReaderError::WrongBuffer);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameReaderError {
    /// Neither the cursor nor either buffer is changed. Create a new reader on
    /// the intended buffer instead; reset does not silently rebind a cursor.
    WrongBuffer,
}

impl fmt::Display for FrameReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("frame-event reader belongs to a different buffer")
    }
}

impl std::error::Error for FrameReaderError {}

/// Frame-event activity since construction or the last metrics reset.
///
/// Counters saturate at [`u64::MAX`]. Clearing events does not reset metrics or
/// count as overflow/expiration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameEventsMetrics {
    /// Sends accepted into the pending buffer.
    pub accepted: u64,
    /// Sends rejected because the pending buffer was full (including capacity zero).
    pub rejected: u64,
    /// Visible events discarded by advancing to another frame.
    ///
    /// Reads do not remove events, so this includes events seen by borrowed
    /// listeners. Events removed by a drain or clear are excluded.
    pub expired: u64,
    /// Largest pending length, including the pending length at the last reset.
    pub pending_high_water_mark: usize,
}

/// Double-buffered events with explicit publication and pending-buffer backpressure.
pub struct FrameEvents<T> {
    visible: Vec<T>,
    pending: Vec<T>,
    capacity: usize,
    metrics: FrameEventsMetrics,
    reader_source: OnceLock<Arc<()>>,
    frame: u64,
    published: u64,
}
impl<T> FrameEvents<T> {
    /// Creates empty buffers, each bounded by `capacity`. Storage grows lazily.
    pub fn new(capacity: usize) -> Self {
        Self {
            visible: Vec::new(),
            pending: Vec::new(),
            capacity,
            metrics: FrameEventsMetrics::default(),
            reader_source: OnceLock::new(),
            frame: 0,
            published: 0,
        }
    }
    /// Queues an event for the next advance, returning it unchanged if pending is full.
    /// Zero capacity always rejects. Visible events do not consume pending capacity.
    pub fn send(&mut self, event: T) -> Result<(), T> {
        if self.is_full() {
            self.metrics.rejected = self.metrics.rejected.saturating_add(1);
            return Err(event);
        }
        self.pending.push(event);
        self.metrics.accepted = self.metrics.accepted.saturating_add(1);
        self.metrics.pending_high_water_mark =
            self.metrics.pending_high_water_mark.max(self.pending.len());
        Ok(())
    }
    /// Borrows published events in send order; repeated reads see the same events.
    pub fn read(&self) -> &[T] {
        &self.visible
    }
    /// Borrows published events in send order without allocating or consuming them.
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.visible.iter()
    }
    /// Creates a cursor starting at the oldest currently visible event.
    ///
    /// Earlier history is excluded from its diagnostics. The first reader for
    /// a buffer allocates one shared identity token; later readers and cursor
    /// operations allocate no storage. The buffer keeps no list of readers.
    ///
    /// ```
    /// use ferravex_engine::frame_events::FrameEvents;
    /// let mut events = FrameEvents::new(2);
    /// let mut reader = events.reader();
    /// events.send("hit").unwrap();
    /// assert!(events.read_with(&mut reader).unwrap().events.is_empty());
    /// events.advance();
    /// assert_eq!(events.read_with(&mut reader).unwrap().events, ["hit"]);
    /// assert!(events.read_with(&mut reader).unwrap().events.is_empty());
    /// assert_eq!(events.read(), ["hit"]); // Other listeners still see it.
    /// ```
    pub fn reader(&self) -> FrameEventReader {
        FrameEventReader {
            source: Arc::clone(self.reader_source.get_or_init(|| Arc::new(()))),
            frame: self.frame,
            next_event: self.visible_start(),
            unread_frame: !self.visible.is_empty(),
        }
    }
    /// Inspects a cursor without changing its position or acknowledging loss.
    ///
    /// An empty buffer can still report missed events. Repeated inspections
    /// preserve the report until a cursor read or reset acknowledges it.
    ///
    /// ```
    /// use ferravex_engine::frame_events::FrameEvents;
    /// let mut events = FrameEvents::new(1);
    /// let mut reader = events.reader();
    /// events.send(7).unwrap();
    /// events.advance();
    /// events.advance(); // Expire 7 and publish an empty frame.
    /// let status = events.reader_status(&reader).unwrap();
    /// assert_eq!((status.frames_behind, status.missed_frames), (2, 1));
    /// assert_eq!((status.unread_events, status.missed_events), (0, 1));
    /// assert_eq!(events.read_with(&mut reader).unwrap().status, status);
    /// assert_eq!(events.reader_status(&reader).unwrap().missed_events, 0);
    /// ```
    pub fn reader_status(
        &self,
        reader: &FrameEventReader,
    ) -> Result<FrameReaderStatus, FrameReaderError> {
        if !self
            .reader_source
            .get()
            .is_some_and(|source| Arc::ptr_eq(source, &reader.source))
        {
            return Err(FrameReaderError::WrongBuffer);
        }
        let start = self.visible_start();
        let frames_behind = self.frame - reader.frame;
        Ok(FrameReaderStatus {
            frames_behind,
            missed_frames: frames_behind.saturating_sub(1)
                + u64::from(frames_behind > 0 && reader.unread_frame),
            missed_events: start.saturating_sub(reader.next_event),
            unread_events: (self.published - reader.next_event.max(start)) as usize,
        })
    }
    /// Borrows and acknowledges all currently available unread events for one cursor.
    ///
    /// Loss is returned alongside the events, then acknowledged, including for
    /// empty reads. Pending events and other readers are unaffected. See
    /// [`Self::reader`] for an example and [`Self::read_up_to`] for a read budget.
    pub fn read_with<'a>(
        &'a self,
        reader: &mut FrameEventReader,
    ) -> Result<FrameEventRead<'a, T>, FrameReaderError> {
        self.read_up_to(reader, usize::MAX)
    }
    /// Borrows and acknowledges up to `limit` unread events in send order.
    ///
    /// The returned status precedes the read. Even a zero limit acknowledges
    /// reported loss, but leaves available events unread. Use [`Self::reader_status`]
    /// for an inspection that never changes the cursor. The returned slice is
    /// acknowledged immediately; processing only part of it does not rewind.
    ///
    /// ```
    /// use ferravex_engine::frame_events::FrameEvents;
    /// let mut events = FrameEvents::new(2);
    /// events.send(1).unwrap();
    /// events.send(2).unwrap();
    /// events.advance();
    /// let mut reader = events.reader();
    /// let batch = events.read_up_to(&mut reader, 1).unwrap();
    /// assert_eq!(batch.events, [1]);
    /// assert_eq!(batch.status.unread_events, 2);
    /// // Destructive drains remove events for every reader. Already-read
    /// // events do not count as missed for this cursor.
    /// drop(events.drain_up_to(1));
    /// assert_eq!(events.read_with(&mut reader).unwrap().events, [2]);
    /// ```
    pub fn read_up_to<'a>(
        &'a self,
        reader: &mut FrameEventReader,
        limit: usize,
    ) -> Result<FrameEventRead<'a, T>, FrameReaderError> {
        let status = self.reader_status(reader)?;
        let start = self.visible.len() - status.unread_events;
        let count = limit.min(status.unread_events);
        self.position_reader(reader, self.visible_start() + (start + count) as u64);
        Ok(FrameEventRead {
            events: &self.visible[start..start + count],
            status,
        })
    }
    /// Returns the previous status and resets a cursor to a retained-data boundary.
    ///
    /// Both destinations acknowledge prior loss without changing events, pending
    /// data, other cursors or metrics. Expired/drained/cleared events cannot be
    /// recovered. A cursor from another buffer is rejected without mutation.
    ///
    /// ```
    /// use ferravex_engine::frame_events::{FrameEvents, FrameReaderPosition};
    /// let mut events = FrameEvents::new(1);
    /// events.send(1).unwrap();
    /// events.advance();
    /// let mut reader = events.reader();
    /// let skipped = events.reset_reader(&mut reader, FrameReaderPosition::Latest).unwrap();
    /// assert_eq!(skipped.unread_events, 1);
    /// assert!(events.read_with(&mut reader).unwrap().events.is_empty());
    /// events.reset_reader(&mut reader, FrameReaderPosition::Oldest).unwrap();
    /// assert_eq!(events.read_with(&mut reader).unwrap().events, [1]);
    /// events.send(2).unwrap();
    /// events.advance();
    /// assert_eq!(events.read_with(&mut reader).unwrap().events, [2]);
    /// ```
    pub fn reset_reader(
        &self,
        reader: &mut FrameEventReader,
        position: FrameReaderPosition,
    ) -> Result<FrameReaderStatus, FrameReaderError> {
        let status = self.reader_status(reader)?;
        let next_event = match position {
            FrameReaderPosition::Oldest => self.visible_start(),
            FrameReaderPosition::Latest => self.published,
        };
        self.position_reader(reader, next_event);
        Ok(status)
    }

    fn visible_start(&self) -> u64 {
        // Visible storage always contains a suffix of the latest publication.
        self.published - self.visible.len() as u64
    }

    fn position_reader(&self, reader: &mut FrameEventReader, next_event: u64) {
        reader.frame = self.frame;
        reader.next_event = next_event;
        reader.unread_frame = next_event < self.published;
    }

    /// Removes published events in send order without allocating or cloning.
    ///
    /// Dropping the iterator discards unyielded visible events. Pending events
    /// remain queued for the next advance, and both buffers retain their storage.
    /// Cursors are not moved: any removed events they have not read are reported
    /// as missed by their next status/read. Already-read events are not missed.
    pub fn drain(&mut self) -> vec::Drain<'_, T> {
        self.visible.drain(..)
    }
    /// Removes up to `limit` oldest visible events without allocating or cloning.
    ///
    /// Dropping the iterator discards the unyielded selected prefix. Unselected
    /// visible events remain available until drained, cleared or expired by the
    /// next advance. Pending events are unaffected; a zero limit removes nothing.
    /// Cursor positions remain valid after prefix removal; unread removed events
    /// are reported as missed, including selected events discarded on iterator drop.
    pub fn drain_up_to(&mut self, limit: usize) -> vec::Drain<'_, T> {
        let count = limit.min(self.visible.len());
        self.visible.drain(..count)
    }
    /// Number of visible events; pending events are excluded.
    pub fn len(&self) -> usize {
        self.visible.len()
    }
    /// Whether the visible buffer is empty, regardless of pending events.
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }
    /// Number of events waiting for the next advance.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    /// Maximum event count in each of the visible and pending buffers.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Number of additional sends that fit before the next advance.
    pub fn remaining_capacity(&self) -> usize {
        self.capacity - self.pending.len()
    }
    /// Whether pending is full and sends will be rejected. True at zero capacity.
    pub fn is_full(&self) -> bool {
        self.pending.len() == self.capacity
    }
    /// Expires the current visible events and publishes pending events in send order.
    /// The buffers are swapped and reused without allocating.
    ///
    /// # Panics
    ///
    /// Panics before changing either buffer if the lifetime advance count or
    /// published event count would exceed [`u64::MAX`]. Cursor sequences never
    /// wrap or saturate into a misleading empty read; metrics still saturate.
    pub fn advance(&mut self) {
        let frame = self.frame.checked_add(1).expect("frame sequence exhausted");
        let published = self
            .published
            .checked_add(self.pending.len() as u64)
            .expect("published event sequence exhausted");
        self.metrics.expired = self
            .metrics
            .expired
            .saturating_add(self.visible.len() as u64);
        self.visible.clear();
        std::mem::swap(&mut self.visible, &mut self.pending);
        self.frame = frame;
        self.published = published;
    }
    /// Returns a copy of the metrics without changing either buffer.
    pub fn metrics(&self) -> FrameEventsMetrics {
        self.metrics
    }
    /// Returns the previous metrics and starts a new observation interval.
    ///
    /// Neither buffer changes. Counters become zero; the pending high-water mark
    /// starts at the current pending length. Subsequent advances still count any
    /// visible events they expire, even if those events predate the reset.
    pub fn reset_metrics(&mut self) -> FrameEventsMetrics {
        std::mem::replace(
            &mut self.metrics,
            FrameEventsMetrics {
                pending_high_water_mark: self.pending.len(),
                ..FrameEventsMetrics::default()
            },
        )
    }
    /// Discards both buffers while retaining their backing storage and metrics.
    /// Readers report unread visible events as missed; unpublished pending events
    /// do not count. Frame/event sequences and reader identities are preserved.
    pub fn clear(&mut self) {
        self.visible.clear();
        self.pending.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listeners_share_stable_frame_and_pending_events_wait() {
        let mut e = FrameEvents::new(2);
        e.send(1).unwrap();
        assert!(e.read().is_empty());
        e.advance();
        assert_eq!(e.read(), [1]);
        e.send(2).unwrap();
        e.send(3).unwrap();
        assert_eq!(e.send(4), Err(4));
        assert_eq!(e.read(), [1]);
        assert_eq!(e.read(), [1]);
        e.advance();
        assert_eq!(e.read(), [2, 3]);
        e.advance();
        assert!(e.read().is_empty());
        e.send(9).unwrap();
        e.clear();
        e.advance();
        assert!(e.read().is_empty());
        assert_eq!(FrameEvents::new(0).send(1), Err(1));
    }

    #[test]
    fn pending_backpressure_is_independent_of_visible_capacity() {
        let mut e = FrameEvents::new(2);
        assert_eq!((e.capacity(), e.remaining_capacity()), (2, 2));
        assert!(!e.is_full());
        e.send(1).unwrap();
        e.send(2).unwrap();
        assert!(e.is_empty());
        assert!(e.is_full());
        assert_eq!(e.send(3), Err(3));
        e.advance();
        assert!(!e.is_full());
        assert_eq!(e.remaining_capacity(), 2);
        e.send(4).unwrap();
        e.send(5).unwrap();
        assert_eq!((e.len(), e.pending_len()), (2, 2));
        assert_eq!(e.send(6), Err(6));
        assert_eq!(e.read(), [1, 2]);
        assert!(e.iter().copied().eq([1, 2]));
        assert_eq!(
            e.metrics(),
            FrameEventsMetrics {
                accepted: 4,
                rejected: 2,
                expired: 0,
                pending_high_water_mark: 2,
            }
        );
        e.advance();
        assert_eq!(e.read(), [4, 5]);
        assert_eq!(e.metrics().expired, 2);
        e.advance();
        assert!(e.is_empty());
        assert_eq!(e.metrics().expired, 4);
        e.advance();
        assert_eq!(e.metrics().expired, 4);
    }

    #[test]
    fn zero_capacity_counts_rejections_without_publishing() {
        let mut e = FrameEvents::new(0);
        assert!(e.is_full());
        assert_eq!((e.capacity(), e.remaining_capacity()), (0, 0));
        assert_eq!(e.send(1), Err(1));
        assert_eq!(e.send(2), Err(2));
        e.advance();
        assert_eq!(e.iter().len(), 0);
        assert_eq!(e.drain().next(), None);
        assert_eq!(e.drain_up_to(usize::MAX).next(), None);
        e.clear();
        assert_eq!(
            e.metrics(),
            FrameEventsMetrics {
                rejected: 2,
                ..FrameEventsMetrics::default()
            }
        );
        assert_eq!(e.reset_metrics().rejected, 2);
        assert_eq!(e.metrics(), FrameEventsMetrics::default());
    }

    #[test]
    fn bounded_drains_consume_only_visible_events_across_frames() {
        let mut e = FrameEvents::new(3);
        for event in 1..=3 {
            e.send(event).unwrap();
        }
        e.advance();
        e.send(4).unwrap();
        assert_eq!(e.drain_up_to(0).len(), 0);
        assert_eq!(e.len(), 3);
        assert!(e.iter().rev().copied().eq([3, 2, 1]));
        {
            let mut drain = e.drain_up_to(2);
            assert_eq!(drain.len(), 2);
            assert_eq!(drain.next(), Some(1));
            // Dropping removes the unyielded selected event, but leaves 3.
        }
        assert_eq!(e.read(), [3]);
        assert_eq!(e.pending_len(), 1);
        assert_eq!(e.remaining_capacity(), 2);
        assert_eq!(e.metrics().expired, 0);
        e.advance();
        assert_eq!(e.metrics().expired, 1);
        assert!(e.drain_up_to(usize::MAX).eq([4]));
        assert!(e.is_empty());
        e.send(5).unwrap();
        e.advance();
        assert!(e.drain().eq([5]));
        e.advance();
        assert!(e.is_empty());
        assert_eq!(e.metrics().expired, 1);
    }

    #[test]
    fn clear_and_metrics_reset_are_independent_of_frame_publication() {
        let mut e = FrameEvents::new(2);
        e.send(1).unwrap();
        e.send(2).unwrap();
        assert_eq!(e.send(9), Err(9));
        e.advance();
        e.send(3).unwrap();
        let observed = e.metrics();
        assert_eq!(e.reset_metrics(), observed);
        assert_eq!(e.read(), [1, 2]);
        assert_eq!(e.pending_len(), 1);
        assert_eq!(
            e.metrics(),
            FrameEventsMetrics {
                pending_high_water_mark: 1,
                ..FrameEventsMetrics::default()
            }
        );
        e.advance();
        assert_eq!(e.read(), [3]);
        assert_eq!(e.metrics().expired, 2);
        e.send(4).unwrap();
        let before_clear = e.metrics();
        e.clear();
        assert_eq!(e.metrics(), before_clear);
        assert!(e.is_empty());
        assert_eq!(e.pending_len(), 0);
        e.advance();
        assert!(e.is_empty());
        assert_eq!(e.metrics(), before_clear);
        e.reset_metrics();
        assert_eq!(e.metrics(), FrameEventsMetrics::default());
    }

    #[test]
    fn owned_drains_and_frame_swaps_reuse_both_buffers() {
        #[derive(Debug, PartialEq)]
        struct Event(u32); // Deliberately not Clone or Copy.
        let mut e = FrameEvents::new(2);
        for frame in 0..2 {
            e.send(Event(frame * 2)).unwrap();
            e.send(Event(frame * 2 + 1)).unwrap();
            e.advance();
        }
        // Both buffers have now been filled once and can be reused.
        let visible_storage = (e.visible.as_ptr(), e.visible.capacity());
        let pending_storage = (e.pending.as_ptr(), e.pending.capacity());
        assert!(e.drain().eq([Event(2), Event(3)]));
        assert_eq!((e.visible.as_ptr(), e.visible.capacity()), visible_storage);
        e.send(Event(4)).unwrap();
        e.send(Event(5)).unwrap();
        e.advance();
        assert_eq!((e.visible.as_ptr(), e.visible.capacity()), pending_storage);
        assert_eq!((e.pending.as_ptr(), e.pending.capacity()), visible_storage);
        assert!(e.drain().eq([Event(4), Event(5)]));
        e.clear();
        assert_eq!((e.visible.as_ptr(), e.visible.capacity()), pending_storage);
        assert_eq!((e.pending.as_ptr(), e.pending.capacity()), visible_storage);
    }

    #[test]
    fn frame_counters_saturate_without_changing_publication() {
        let mut e = FrameEvents::new(1);
        e.metrics.accepted = u64::MAX;
        e.metrics.rejected = u64::MAX;
        e.metrics.expired = u64::MAX;
        e.send(1).unwrap();
        assert_eq!(e.send(2), Err(2));
        e.advance();
        assert_eq!(e.read(), [1]);
        e.advance();
        assert!(e.is_empty());
        assert_eq!(e.metrics().accepted, u64::MAX);
        assert_eq!(e.metrics().rejected, u64::MAX);
        assert_eq!(e.metrics().expired, u64::MAX);
    }

    #[test]
    fn independent_readers_observe_each_publication_once_in_send_order() {
        let mut e = FrameEvents::new(3);
        let mut first = e.reader();
        let mut second = first.clone();
        e.send(1).unwrap();
        e.send(2).unwrap();
        assert_eq!(e.reader_status(&first), Ok(FrameReaderStatus::default()));
        assert!(e.read_with(&mut first).unwrap().events.is_empty());
        e.advance();
        e.send(3).unwrap();
        let observed = e.reader_status(&first).unwrap();
        assert_eq!(
            observed,
            FrameReaderStatus {
                frames_behind: 1,
                unread_events: 2,
                ..FrameReaderStatus::default()
            }
        );
        assert_eq!(e.reader_status(&first).unwrap(), observed);
        let batch = e.read_up_to(&mut first, 1).unwrap();
        assert_eq!(batch.status, observed);
        assert_eq!(batch.events, [1]);
        assert_eq!(e.read_with(&mut second).unwrap().events, [1, 2]);
        assert_eq!(e.read_with(&mut first).unwrap().events, [2]);
        assert!(e.read_with(&mut first).unwrap().events.is_empty());
        assert_eq!(e.read(), [1, 2]);
        assert_eq!(e.pending_len(), 1);
        e.advance();
        for reader in [&mut first, &mut second] {
            let batch = e.read_with(reader).unwrap();
            assert_eq!(batch.events, [3]);
            assert_eq!(batch.status.frames_behind, 1);
            assert_eq!(batch.status.missed_frames, 0);
            assert_eq!(batch.status.missed_events, 0);
        }
    }

    #[test]
    fn expired_data_is_distinct_from_an_empty_frame_and_loss_is_acknowledged_once() {
        let mut e = FrameEvents::new(2);
        let mut reader = e.reader();
        e.send(10).unwrap();
        e.send(20).unwrap();
        e.advance();
        e.advance();
        assert!(e.is_empty());
        let status = e.reader_status(&reader).unwrap();
        assert_eq!(
            status,
            FrameReaderStatus {
                frames_behind: 2,
                missed_frames: 1,
                missed_events: 2,
                unread_events: 0,
            }
        );
        let batch = e.read_with(&mut reader).unwrap();
        assert_eq!(batch.status, status);
        assert!(batch.events.is_empty());
        assert_eq!(e.reader_status(&reader), Ok(FrameReaderStatus::default()));
        // Even empty publications are frames; the last acknowledged one is excluded.
        e.advance();
        e.advance();
        e.advance();
        let batch = e.read_with(&mut reader).unwrap();
        assert!(batch.events.is_empty());
        assert_eq!(batch.status.frames_behind, 3);
        assert_eq!(batch.status.missed_frames, 2);
        assert_eq!(batch.status.missed_events, 0);
    }

    #[test]
    fn partial_reads_count_an_unfinished_frame_when_it_expires() {
        let mut e = FrameEvents::new(3);
        for value in 1..=3 {
            e.send(value).unwrap();
        }
        e.advance();
        let mut partial = e.reader();
        let mut complete = partial.clone();
        assert_eq!(e.read_up_to(&mut partial, 1).unwrap().events, [1]);
        assert_eq!(e.read_with(&mut complete).unwrap().events, [1, 2, 3]);
        e.send(4).unwrap();
        e.advance();
        let batch = e.read_with(&mut partial).unwrap();
        assert_eq!(batch.events, [4]);
        assert_eq!(
            batch.status,
            FrameReaderStatus {
                frames_behind: 1,
                missed_frames: 1,
                missed_events: 2,
                unread_events: 1,
            }
        );
        let batch = e.read_with(&mut complete).unwrap();
        assert_eq!(batch.events, [4]);
        assert_eq!(batch.status.missed_frames, 0);
        assert_eq!(batch.status.missed_events, 0);
    }

    #[test]
    fn prefix_drains_preserve_positions_and_report_only_unread_removals() {
        let mut e = FrameEvents::new(4);
        for value in 1..=4 {
            e.send(value).unwrap();
        }
        e.advance();
        e.send(5).unwrap();
        let mut partial = e.reader();
        let mut untouched = partial.clone();
        let mut complete = partial.clone();
        assert_eq!(e.read_up_to(&mut partial, 1).unwrap().events, [1]);
        assert_eq!(e.read_with(&mut complete).unwrap().events, [1, 2, 3, 4]);
        drop(e.drain_up_to(0));
        assert_eq!(e.reader_status(&untouched).unwrap().missed_events, 0);
        {
            let mut drain = e.drain_up_to(3);
            assert_eq!(drain.next(), Some(1));
            // Two more events are removed on drop, even though not yielded.
        }
        assert_eq!(e.read(), [4]);
        let batch = e.read_with(&mut partial).unwrap();
        assert_eq!(batch.events, [4]);
        assert_eq!(batch.status.missed_events, 2);
        assert_eq!(batch.status.missed_frames, 0);
        assert_eq!(batch.status.frames_behind, 0);
        assert_eq!(e.reader_status(&complete), Ok(FrameReaderStatus::default()));
        drop(e.drain());
        let batch = e.read_with(&mut untouched).unwrap();
        assert!(batch.events.is_empty());
        assert_eq!(batch.status.missed_events, 4);
        assert_eq!(e.reader_status(&partial), Ok(FrameReaderStatus::default()));
        assert_eq!(e.pending_len(), 1);
        e.advance();
        assert_eq!(e.metrics().expired, 0);
        for reader in [&mut partial, &mut untouched, &mut complete] {
            let batch = e.read_with(reader).unwrap();
            assert_eq!(batch.events, [5]);
            assert_eq!(batch.status.missed_events, 0);
            assert_eq!(batch.status.missed_frames, 0);
        }
    }

    #[test]
    fn zero_budget_acknowledges_loss_but_never_consumes_available_events() {
        let mut e = FrameEvents::new(2);
        e.send(1).unwrap();
        e.send(2).unwrap();
        e.advance();
        let mut reader = e.reader();
        drop(e.drain_up_to(1));
        let observed = e.reader_status(&reader).unwrap();
        assert_eq!((observed.missed_events, observed.unread_events), (1, 1));
        let batch = e.read_up_to(&mut reader, 0).unwrap();
        assert!(batch.events.is_empty());
        assert_eq!(batch.status, observed);
        let status = e.reader_status(&reader).unwrap();
        assert_eq!((status.missed_events, status.unread_events), (0, 1));
        assert_eq!(e.read_up_to(&mut reader, usize::MAX).unwrap().events, [2]);
    }

    #[test]
    fn clear_reports_only_unread_publications_and_metrics_resets_do_not_erase_lag() {
        let mut e = FrameEvents::new(2);
        let mut reader = e.reader();
        e.send(1).unwrap();
        e.send(2).unwrap();
        assert_eq!(e.send(9), Err(9));
        e.advance();
        assert_eq!(e.read_up_to(&mut reader, 1).unwrap().events, [1]);
        e.send(3).unwrap();
        e.clear();
        let status = e.reader_status(&reader).unwrap();
        assert_eq!(status.missed_events, 1);
        assert_eq!(status.missed_frames, 0);
        assert_eq!(status.unread_events, 0);
        e.reset_metrics();
        assert_eq!(e.reader_status(&reader).unwrap(), status);
        e.clear();
        assert_eq!(e.reader_status(&reader).unwrap(), status);
        e.send(4).unwrap();
        e.advance();
        let batch = e.read_with(&mut reader).unwrap();
        assert_eq!(batch.events, [4]);
        assert_eq!(batch.status.missed_events, 1);
        assert_eq!(batch.status.missed_frames, 1);
        assert_eq!(e.metrics().expired, 0);
    }

    #[test]
    fn reset_replays_only_retained_events_or_skips_to_future_publications() {
        let mut e = FrameEvents::new(3);
        for value in 1..=3 {
            e.send(value).unwrap();
        }
        e.advance();
        e.send(4).unwrap();
        let mut reader = e.reader();
        let other = reader.clone();
        assert_eq!(e.read_up_to(&mut reader, 2).unwrap().events, [1, 2]);
        drop(e.drain_up_to(1));
        let metrics = e.metrics();
        let other_status = e.reader_status(&other).unwrap();
        let previous = e.reader_status(&reader).unwrap();
        assert_eq!(
            e.reset_reader(&mut reader, FrameReaderPosition::Oldest),
            Ok(previous)
        );
        assert_eq!(e.read_with(&mut reader).unwrap().events, [2, 3]);
        e.reset_reader(&mut reader, FrameReaderPosition::Oldest)
            .unwrap();
        let skipped = e
            .reset_reader(&mut reader, FrameReaderPosition::Latest)
            .unwrap();
        assert_eq!(skipped.unread_events, 2);
        assert!(e.read_with(&mut reader).unwrap().events.is_empty());
        assert_eq!(e.reader_status(&other).unwrap(), other_status);
        assert_eq!(e.read(), [2, 3]);
        assert_eq!(e.pending_len(), 1);
        assert_eq!(e.metrics(), metrics);
        e.advance();
        assert_eq!(e.read_with(&mut reader).unwrap().events, [4]);
        e.advance();
        // Reset also returns and acknowledges prior loss, even with no retained data.
        let mut other = other;
        let status = e.reader_status(&other).unwrap();
        assert_eq!(status.missed_events, 4);
        assert_eq!(
            e.reset_reader(&mut other, FrameReaderPosition::Oldest),
            Ok(status)
        );
        assert_eq!(e.reader_status(&other), Ok(FrameReaderStatus::default()));
    }

    #[test]
    fn wrong_buffer_errors_are_transactional_and_buffer_moves_preserve_identity() {
        let mut first = FrameEvents::new(1);
        let mut second = FrameEvents::new(1);
        first.send(1).unwrap();
        first.advance();
        second.send(2).unwrap();
        second.advance();
        let mut reader = first.reader();
        let expected = first.reader_status(&reader).unwrap();
        assert_eq!(
            second.reader_status(&reader),
            Err(FrameReaderError::WrongBuffer)
        );
        // Rejection does not allocate an identity token in the wrong buffer.
        assert!(second.reader_source.get().is_none());
        let _second_reader = second.reader();
        assert_eq!(
            second.read_with(&mut reader).unwrap_err(),
            FrameReaderError::WrongBuffer
        );
        assert_eq!(
            second.read_up_to(&mut reader, 0).unwrap_err(),
            FrameReaderError::WrongBuffer
        );
        for position in [FrameReaderPosition::Oldest, FrameReaderPosition::Latest] {
            assert_eq!(
                second.reset_reader(&mut reader, position),
                Err(FrameReaderError::WrongBuffer)
            );
            assert_eq!(first.reader_status(&reader), Ok(expected));
        }
        assert_eq!(second.read(), [2]);
        std::mem::swap(&mut first, &mut second);
        assert_eq!(
            first.reader_status(&reader),
            Err(FrameReaderError::WrongBuffer)
        );
        assert_eq!(second.read_with(&mut reader).unwrap().events, [1]);
        drop(second);
        // A cursor outliving its buffer must not attach to a replacement.
        assert_eq!(
            FrameEvents::<i32>::new(1).reader_status(&reader),
            Err(FrameReaderError::WrongBuffer)
        );
    }

    #[test]
    fn late_readers_ignore_old_history_and_lagging_readers_do_not_extend_retention() {
        let mut e = FrameEvents::new(2);
        let mut lagging = e.reader();
        for frame in 0..100 {
            e.send(frame * 2).unwrap();
            e.send(frame * 2 + 1).unwrap();
            assert_eq!(e.send(-1), Err(-1));
            assert!(e.len() + e.pending_len() <= 2 * e.capacity());
            e.advance();
            assert_eq!(e.read(), [frame * 2, frame * 2 + 1]);
        }
        let mut late = e.reader();
        let batch = e.read_with(&mut late).unwrap();
        assert_eq!(batch.events, [198, 199]);
        assert_eq!(batch.status.missed_events, 0);
        assert_eq!(batch.status.frames_behind, 0);
        let batch = e.read_with(&mut lagging).unwrap();
        assert_eq!(batch.events, [198, 199]);
        assert_eq!(batch.status.frames_behind, 100);
        assert_eq!(batch.status.missed_frames, 99);
        assert_eq!(batch.status.missed_events, 198);
        assert_eq!(e.metrics().rejected, 100);
    }

    #[test]
    fn reader_identity_is_lazy_and_zero_capacity_still_tracks_empty_frames() {
        let mut e = FrameEvents::<u8>::new(0);
        assert_eq!(e.send(1), Err(1));
        e.advance();
        e.clear();
        e.reset_metrics();
        assert!(e.reader_source.get().is_none());
        let mut reader = e.reader();
        let another = e.reader();
        assert!(Arc::ptr_eq(&reader.source, &another.source));
        e.advance();
        e.advance();
        let batch = e.read_with(&mut reader).unwrap();
        assert!(batch.events.is_empty());
        assert_eq!(batch.status.frames_behind, 2);
        assert_eq!(batch.status.missed_frames, 1);
        assert_eq!(batch.status.missed_events, 0);
        assert_eq!(e.visible.capacity(), 0);
        assert_eq!(e.pending.capacity(), 0);
    }

    #[test]
    fn sequence_exhaustion_cannot_alias_cursors_or_partially_publish() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        for exhaust_frames in [false, true] {
            let mut e = FrameEvents::new(1);
            e.published = u64::MAX - 1;
            if exhaust_frames {
                e.frame = u64::MAX - 1;
            }
            let mut reader = e.reader();
            e.send(1).unwrap();
            e.advance();
            assert_eq!(e.read_with(&mut reader).unwrap().events, [1]);
            e.send(2).unwrap();
            let status = e.reader_status(&reader).unwrap();
            let metrics = e.metrics();
            let frame = e.frame;
            assert!(catch_unwind(AssertUnwindSafe(|| e.advance())).is_err());
            assert_eq!(e.frame, frame);
            assert_eq!(e.published, u64::MAX);
            assert_eq!(e.read(), [1]);
            assert_eq!(e.pending, [2]);
            assert_eq!(e.reader_status(&reader), Ok(status));
            assert_eq!(e.metrics(), metrics);
        }
    }
}
