//! Bounded ordered event delivery over an unreliable transport. This is not encryption.
use std::collections::{BTreeMap, VecDeque};

pub const EVENT_LIMIT: usize = 256;
pub const EVENT_BYTES: usize = 900;
const RETRY_MS: u64 = 100;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPacket {
    pub sequence: u64,
    pub payload: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Full,
    Oversized,
    SequenceExhausted,
    OutsideWindow,
}
struct Pending {
    packet: EventPacket,
    last_sent: Option<u64>,
}
impl Pending {
    fn is_due(&self, now: u64) -> bool {
        self.last_sent
            .is_none_or(|last| now.saturating_sub(last) >= RETRY_MS)
    }
}

/// Accepted work and remaining eligible work from [`EventSender::send_due`].
///
/// Queued packets stay pending until individually acknowledged, even after a
/// successful send. `remaining_due` excludes accepted packets waiting to retry.
///
/// ```rust
/// use ferravex_net::reliable::EventSender;
/// let mut sender = EventSender::default();
/// sender.queue(b"event")?;
/// let report = sender.send_due(0, 1, 5, |_| true);
/// assert_eq!((report.sent, report.payload_bytes, report.remaining_due), (1, 5, 0));
/// assert_eq!(sender.pending(), 1); // Acceptance is not a remote ACK.
/// # Ok::<(), ferravex_net::reliable::Error>(())
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SendReport {
    /// Packets whose callback returned `true` during this call.
    pub sent: usize,
    /// Sum of accepted payload lengths; excludes sequence and encoded overhead.
    pub payload_bytes: usize,
    /// Packets still eligible at the supplied time, including a refused or
    /// over-budget packet and the untouched due tail. At most [`EVENT_LIMIT`].
    pub remaining_due: usize,
}

#[derive(Default)]
pub struct EventSender {
    next: u64,
    pending: VecDeque<Pending>,
}
impl EventSender {
    pub fn queue(&mut self, payload: &[u8]) -> Result<u64, Error> {
        if payload.len() > EVENT_BYTES {
            return Err(Error::Oversized);
        }
        if self.pending.len() >= EVENT_LIMIT {
            return Err(Error::Full);
        }
        let sequence = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::SequenceExhausted)?;
        self.pending.push_back(Pending {
            packet: EventPacket {
                sequence,
                payload: payload.to_vec(),
            },
            last_sent: None,
        });
        Ok(sequence)
    }
    /// Returns at most `budget` events. Call with a monotonic millisecond clock.
    pub fn due(&mut self, now: u64, budget: usize) -> Vec<EventPacket> {
        self.pending
            .iter_mut()
            .filter(|p| p.is_due(now))
            .take(budget.min(EVENT_LIMIT))
            .map(|p| {
                p.last_sent = Some(now);
                p.packet.clone()
            })
            .collect()
    }
    /// Offers due packets by immutable borrow, in sequence order, without
    /// allocating or cloning. Only a callback returning `true` marks its packet
    /// sent at `now`; `false` refuses that packet and stops the batch. Translate
    /// transport errors/backpressure to `false` and retain any error externally.
    /// A successful send is not an ACK: call [`Self::acknowledge`] separately.
    ///
    /// At most `packet_budget` callbacks run (also bounded by [`EVENT_LIMIT`]).
    /// `byte_budget` counts **payload bytes only**, excluding sequence numbers,
    /// lengths, session IDs and all transport/protocol overhead. The first due
    /// packet that cannot fit stops the batch without invoking the callback;
    /// later smaller packets are not skipped ahead. Empty payloads fit a zero
    /// byte budget but still consume one packet. Both budgets count accepted
    /// work; a refusal is the final callback. The report counts all remaining
    /// due packets, even after an early stop. Scans are bounded by the window.
    ///
    /// Supply monotonic milliseconds from one clock domain for both send APIs.
    /// Each accepted packet becomes due again after 100 ms. As with [`Self::due`],
    /// a backwards time uses saturating subtraction, delaying retries; no clock
    /// is read or validated here. All acceptances in a batch use this one `now`.
    ///
    /// The mutable sender borrow prevents safe reentrancy, including queueing or
    /// ACKing this sender from the callback. Borrowed packets cannot be retained
    /// beyond their callback; buffer/encode synchronously or explicitly copy.
    /// On an unwinding callback panic, earlier accepted timestamps persist, but
    /// the current packet and untouched tail keep their original timestamps.
    /// Callback side effects are not rolled back: a send followed by `false` or
    /// panic may be retried. With aborting panics the process does not resume.
    ///
    /// This recipe drives the existing receiver with supplied times, local
    /// refusal, loss of the first accepted packet, a retry and individual ACKs.
    /// The receiver and its returned delivery vectors may allocate.
    ///
    /// ```rust
    /// use ferravex_net::reliable::{EventReceiver, EventSender};
    /// let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
    /// sender.queue(b"first")?;
    /// sender.queue(b"second")?;
    /// let report = sender.send_due(0, 2, 11, |packet| {
    ///     packet.sequence == 0 // Accept zero (lost in transit); refuse one.
    /// });
    /// assert_eq!((report.sent, report.remaining_due), (1, 1));
    ///
    /// let mut ack = None;
    /// sender.send_due(1, 2, 11, |packet| {
    ///     assert_eq!(packet.sequence, 1); // Refused packet is immediately due.
    ///     assert!(receiver.receive(packet).unwrap().is_empty()); // Waits for zero.
    ///     ack = Some(packet.sequence); // ACK only a successful receive.
    ///     true
    /// });
    /// assert!(sender.acknowledge(ack.unwrap()));
    /// assert_eq!(sender.pending(), 1); // ACK one does not hide missing zero.
    /// assert_eq!(sender.send_due(99, 2, 11, |_| unreachable!()).sent, 0);
    /// sender.send_due(100, 2, 11, |packet| {
    ///     let delivered = receiver.receive(packet).unwrap();
    ///     assert_eq!(delivered.len(), 2);
    ///     assert_eq!(delivered[0].payload, b"first");
    ///     assert_eq!(delivered[1].payload, b"second");
    ///     ack = Some(packet.sequence);
    ///     true
    /// });
    /// assert!(sender.acknowledge(ack.unwrap()));
    /// assert_eq!(sender.pending(), 0);
    /// # Ok::<(), ferravex_net::reliable::Error>(())
    /// ```
    pub fn send_due(
        &mut self,
        now: u64,
        packet_budget: usize,
        byte_budget: usize,
        mut accept: impl FnMut(&EventPacket) -> bool,
    ) -> SendReport {
        let mut report = SendReport::default();
        for pending in &mut self.pending {
            if !pending.is_due(now) {
                continue;
            }
            let bytes = pending.packet.payload.len();
            if report.sent == packet_budget
                || bytes > byte_budget - report.payload_bytes
                || !accept(&pending.packet)
            {
                break;
            }
            pending.last_sent = Some(now);
            report.sent += 1;
            report.payload_bytes += bytes;
        }
        report.remaining_due = self.pending.iter().filter(|p| p.is_due(now)).count();
        report
    }
    /// Acknowledgments are individual, so loss of an earlier event cannot be hidden by a newer ACK.
    pub fn acknowledge(&mut self, sequence: u64) -> bool {
        if let Some(index) = self
            .pending
            .iter()
            .position(|p| p.packet.sequence == sequence)
        {
            self.pending.remove(index);
            true
        } else {
            false
        }
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}
/// Successful admission, and permission to ACK this packet's individual sequence.
///
/// Both variants are successful receipts. Errors from [`EventReceiver::admit`]
/// must not be ACKed. Duplicate sequences never replace the first retained bytes.
/// See that method's runnable loss/retry/ACK recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receipt {
    /// A new payload was copied into bounded receiver storage.
    Retained,
    /// The sequence was already retained or delivered; no bytes were changed.
    Duplicate,
}

/// Accepted delivery prefix and exact remaining receiver work.
///
/// ```rust
/// use ferravex_net::reliable::{DeliveryReport, EventReceiver};
/// let report = EventReceiver::default().drain_into(&mut [], 0);
/// assert_eq!(report, DeliveryReport::default());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeliveryReport {
    /// Packets accepted by the destination during this call.
    pub delivered: usize,
    /// Accepted payload bytes, excluding protocol/transport overhead.
    pub payload_bytes: usize,
    /// Contiguous packets ready at `next_sequence`, including a refused head.
    pub remaining_ready: usize,
    /// All retained packets, including those waiting for a missing sequence.
    pub buffered: usize,
}

#[derive(Default)]
pub struct EventReceiver {
    next: u64,
    buffered: BTreeMap<u64, Vec<u8>>,
}
impl EventReceiver {
    /// Retains a receipt independently of delivery. ACK only `Ok`, including
    /// duplicates; `Err` retains nothing new and leaves all admission state intact.
    /// `capacity.min(EVENT_LIMIT)` caps the total retained packet count for this
    /// admission, not just new work. Lowering it never evicts existing receipts.
    /// Duplicates still succeed at zero/full capacity. Admission copies a fresh
    /// payload and may allocate both payload and map storage; capacity is a logical
    /// bound, not preallocation or an allocation-failure guarantee.
    ///
    /// Validation order is oversized payload, already-delivered duplicate,
    /// outside the forward 256-sequence window, reserved `u64::MAX`, buffered
    /// duplicate, then capacity. `u64::MAX - 1` is the final admissible sequence,
    /// matching the sender; delivery advances to `u64::MAX` without wrapping.
    /// Retrying a future/outside-window packet never advances the window.
    ///
    /// First receipt wins on conflicting bytes for a buffered duplicate. For
    /// already-delivered sequences bytes are not retained for comparison. Even
    /// duplicates must satisfy the payload size bound. This preserves legacy
    /// duplicate handling; it does not authenticate content. Validate session and
    /// application payload before admission. A window containing later packets
    /// needs space for its missing head: use the full window capacity or reserve
    /// that space when choosing a smaller cap. An ACK allows the sender to forget
    /// data, so keep the receiver alive until delivery completes. Replacing it
    /// with `default()` discards all work and resets to sequence zero; do that only
    /// at a new externally validated transport session rejecting stale packets.
    ///
    /// This supplied-time recipe loses the first packet, retains/ACKs the second
    /// while delivery is blocked, retries the first and drains into one owned slot.
    /// A full destination never discards the remaining tail. ACKs are individual
    /// and applied after the sender callback releases its borrow.
    ///
    /// ```rust
    /// use ferravex_net::reliable::{EventReceiver, EventSender, Receipt, EVENT_LIMIT};
    /// let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
    /// sender.queue(b"first")?;
    /// sender.queue(b"second")?;
    /// sender.send_due(0, 1, 5, |_| true); // Accepted locally, lost in transit.
    /// let mut ack = None;
    /// sender.send_due(1, 1, 6, |packet| {
    ///     assert_eq!(receiver.admit(packet, EVENT_LIMIT), Ok(Receipt::Retained));
    ///     ack = Some(packet.sequence);
    ///     true
    /// });
    /// assert!(sender.acknowledge(ack.take().unwrap()));
    /// assert_eq!(receiver.drain_into(&mut [], 99).buffered, 1);
    /// assert_eq!(sender.send_due(99, 2, 99, |_| unreachable!()).sent, 0);
    /// sender.send_due(100, 1, 5, |packet| {
    ///     receiver.admit(packet, EVENT_LIMIT).unwrap();
    ///     ack = Some(packet.sequence);
    ///     true
    /// });
    /// assert!(sender.acknowledge(ack.unwrap()));
    /// let mut slot = [None];
    /// let report = receiver.drain_into(&mut slot, 5);
    /// assert_eq!((report.delivered, report.remaining_ready, report.buffered), (1, 1, 1));
    /// assert_eq!(slot[0].take().unwrap().payload, b"first");
    /// receiver.drain_into(&mut slot, 6);
    /// assert_eq!(slot[0].take().unwrap().payload, b"second");
    /// assert_eq!((sender.pending(), receiver.buffered()), (0, 0));
    /// # Ok::<(), ferravex_net::reliable::Error>(())
    /// ```
    pub fn admit(&mut self, packet: &EventPacket, capacity: usize) -> Result<Receipt, Error> {
        if packet.payload.len() > EVENT_BYTES {
            return Err(Error::Oversized);
        }
        if packet.sequence < self.next {
            return Ok(Receipt::Duplicate);
        }
        if packet.sequence - self.next >= EVENT_LIMIT as u64 {
            return Err(Error::OutsideWindow);
        }
        if packet.sequence == u64::MAX {
            return Err(Error::SequenceExhausted);
        }
        if self.buffered.contains_key(&packet.sequence) {
            return Ok(Receipt::Duplicate);
        }
        if self.buffered.len() >= capacity.min(EVENT_LIMIT) {
            return Err(Error::Full);
        }
        self.buffered
            .insert(packet.sequence, packet.payload.clone());
        Ok(Receipt::Retained)
    }

    /// Moves an ordered prefix into consecutive `None` slots starting at zero.
    /// Stops at the first occupied slot, missing sequence or payload that exceeds
    /// the remaining byte budget. Never overwrites a slot, skips a head, or clears
    /// any untouched output tail. `output.len()` is the packet budget; empty
    /// payloads cost zero bytes but one slot. The report describes only this call.
    /// `next_sequence` advances only after a slot owns its payload.
    ///
    /// No payload clone or new allocation occurs in this method; map nodes can be
    /// deallocated. Ownership moves to the caller, who must consume/retain it.
    /// The first admission's payload/map allocation is a separate cost.
    ///
    /// ```rust
    /// use ferravex_net::reliable::{EventPacket, EventReceiver, EVENT_LIMIT};
    /// let mut receiver = EventReceiver::default();
    /// receiver.admit(&EventPacket { sequence: 0, payload: vec![7] }, EVENT_LIMIT)?;
    /// let mut output = [None, None];
    /// assert_eq!(receiver.drain_into(&mut output, 0).remaining_ready, 1);
    /// assert_eq!(receiver.drain_into(&mut output, 1).delivered, 1);
    /// assert_eq!(output[0].take().unwrap().payload, [7]);
    /// assert!(output[1].is_none());
    /// # Ok::<(), ferravex_net::reliable::Error>(())
    /// ```
    pub fn drain_into(
        &mut self,
        output: &mut [Option<EventPacket>],
        byte_budget: usize,
    ) -> DeliveryReport {
        let mut report = DeliveryReport::default();
        for slot in output.iter_mut().take(EVENT_LIMIT) {
            if slot.is_some() {
                break;
            }
            let Some(payload) = self.buffered.get(&self.next) else {
                break;
            };
            let bytes = payload.len();
            if bytes > byte_budget - report.payload_bytes {
                break;
            }
            let sequence = self.next;
            let payload = self.buffered.remove(&sequence).expect("present head");
            *slot = Some(EventPacket { sequence, payload });
            self.next += 1; // Admission excludes MAX, so this cannot wrap.
            report.delivered += 1;
            report.payload_bytes += bytes;
        }
        self.finish_delivery(report)
    }

    /// Offers an ordered prefix by borrowed sequence/payload. Only `true` consumes
    /// the head and advances `next_sequence`; `false` stops with head/tail intact.
    /// Both budgets count accepted packets/payload bytes. A too-large head stops
    /// before the callback; no smaller tail is skipped. Empty payloads cost a
    /// packet but zero bytes. At most EVENT_LIMIT packets are visited per call.
    /// The callback must synchronously consume the bytes or explicitly copy them;
    /// borrowed data cannot outlive it. No internal allocation or cloning occurs.
    ///
    /// On native unwinding, earlier accepted deliveries persist; the panicking
    /// head and untouched tail remain retained with the same `next_sequence`.
    /// External callback side effects are not rolled back: consuming then refusing
    /// or panicking may repeat those effects on retry. Aborting panics do not
    /// resume. The exclusive borrow prevents safe same-receiver reentrancy.
    ///
    /// ```rust
    /// use ferravex_net::reliable::{EventPacket, EventReceiver, EVENT_LIMIT};
    /// let mut receiver = EventReceiver::default();
    /// receiver.admit(&EventPacket { sequence: 0, payload: vec![3] }, EVENT_LIMIT)?;
    /// assert_eq!(receiver.drain_ready(1, 1, |_, _| false).remaining_ready, 1);
    /// let mut applied = 0;
    /// let report = receiver.drain_ready(1, 1, |sequence, bytes| {
    ///     assert_eq!((sequence, bytes), (0, [3].as_slice()));
    ///     applied += 1;
    ///     true
    /// });
    /// assert_eq!((applied, report.delivered), (1, 1));
    /// # Ok::<(), ferravex_net::reliable::Error>(())
    /// ```
    pub fn drain_ready(
        &mut self,
        packet_budget: usize,
        byte_budget: usize,
        mut accept: impl FnMut(u64, &[u8]) -> bool,
    ) -> DeliveryReport {
        let mut report = DeliveryReport::default();
        while report.delivered < packet_budget.min(EVENT_LIMIT) {
            let Some(payload) = self.buffered.get(&self.next) else {
                break;
            };
            let bytes = payload.len();
            if bytes > byte_budget - report.payload_bytes || !accept(self.next, payload) {
                break;
            }
            self.buffered.remove(&self.next);
            self.next += 1;
            report.delivered += 1;
            report.payload_bytes += bytes;
        }
        self.finish_delivery(report)
    }

    fn finish_delivery(&self, mut report: DeliveryReport) -> DeliveryReport {
        report.buffered = self.buffered.len();
        report.remaining_ready = self
            .buffered
            .keys()
            .enumerate()
            .take_while(|(index, sequence)| **sequence - self.next == *index as u64)
            .count();
        report
    }

    /// ACK only on success, including duplicates. Delivery is strictly once and ordered.
    /// The receive window bounds memory even when event zero never arrives.
    ///
    /// Allocating compatibility wrapper: admits at full EVENT_LIMIT capacity and
    /// drains every ready packet, including packets retained by `admit`. A valid
    /// duplicate can therefore trigger delivery of a previously retained prefix.
    /// An admission error drains nothing. Mixing this with budgeted drains shares
    /// one cursor/queue; returned packets are consumed and must be handled by the
    /// caller. First receipt wins conflicting duplicate bytes, as before.
    /// Reserved sequence MAX now fails without removing data or wrapping the cursor.
    pub fn receive(&mut self, packet: &EventPacket) -> Result<Vec<EventPacket>, Error> {
        self.admit(packet, EVENT_LIMIT)?;
        let mut delivered = Vec::new();
        // Reserve output ownership before consuming each packet. Allocation
        // failures have the standard Vec behavior, not a recoverable Error.
        while self.buffered.contains_key(&self.next) {
            delivered.reserve(1);
            let sequence = self.next;
            let payload = self.buffered.remove(&sequence).expect("present head");
            delivered.push(EventPacket { sequence, payload });
            self.next += 1;
        }
        Ok(delivered)
    }
    pub fn next_sequence(&self) -> u64 {
        self.next
    }
    pub fn contains_buffered(&self, sequence: u64) -> bool {
        self.buffered.contains_key(&sequence)
    }
    pub fn buffered(&self) -> usize {
        self.buffered.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_reordered_and_duplicate_events_deliver_once_in_order() {
        let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
        for i in 0..8 {
            sender.queue(&[i]).unwrap();
        }
        let packets = sender.due(0, 8);
        let mut delivered = Vec::new();
        for i in [7, 2, 2, 5, 1, 4, 6, 3] {
            delivered.extend(receiver.receive(&packets[i]).unwrap());
            sender.acknowledge(i as u64);
        }
        assert!(delivered.is_empty());
        assert_eq!(sender.pending(), 1);
        assert!(sender.due(99, 8).is_empty());
        let retry = sender.due(100, 8);
        delivered.extend(receiver.receive(&retry[0]).unwrap());
        sender.acknowledge(0);
        assert_eq!(
            delivered.iter().map(|p| p.payload[0]).collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
        assert!(receiver.receive(&retry[0]).unwrap().is_empty());
        assert_eq!(sender.pending(), 0);
    }
    #[test]
    fn flow_control_bounds_memory_and_rejects_future_ack() {
        let mut sender = EventSender::default();
        for _ in 0..EVENT_LIMIT {
            sender.queue(&[1]).unwrap();
        }
        assert_eq!(sender.queue(&[]), Err(Error::Full));
        assert!(!sender.acknowledge(9999));
        assert_eq!(sender.due(0, 3).len(), 3);
        let mut receiver = EventReceiver::default();
        assert_eq!(
            receiver.receive(&EventPacket {
                sequence: 9999,
                payload: vec![]
            }),
            Err(Error::OutsideWindow)
        );
        assert_eq!(receiver.buffered(), 0);
    }
    #[test]
    fn borrowed_send_preserves_sequence_exhaustion_and_default_reset() {
        let mut sender = EventSender {
            next: u64::MAX - 1,
            ..Default::default()
        };
        assert_eq!(sender.queue(b"last"), Ok(u64::MAX - 1));
        assert_eq!(sender.queue(b"never"), Err(Error::SequenceExhausted));
        assert_eq!(sender.next, u64::MAX);
        assert_eq!(sender.pending(), 1);
        assert_eq!(sender.send_due(500, 1, 4, |_| false).remaining_due, 1);
        assert_eq!(sender.pending[0].last_sent, None);
        assert_eq!(sender.send_due(500, 1, 4, |_| true).sent, 1);
        assert_eq!(sender.pending[0].last_sent, Some(500));
        assert_eq!(sender.send_due(599, 1, 4, |_| unreachable!()).sent, 0);
        assert_eq!(sender.send_due(600, 1, 4, |_| true).sent, 1);
        assert!(sender.acknowledge(u64::MAX - 1));
        assert_eq!(
            sender.queue(b"still exhausted"),
            Err(Error::SequenceExhausted)
        );
        // Existing reset idiom: a new transport session must also reject old ACKs.
        sender = EventSender::default();
        assert_eq!(sender.pending(), 0);
        assert_eq!(sender.queue(b"new session"), Ok(0));
        assert_eq!(sender.send_due(0, 1, 11, |_| true).sent, 1);
    }
}

#[cfg(test)]
mod journey400_312_boundaries {
    use super::*;

    #[test]
    fn final_sequences_deliver_without_wrap_and_reserved_max_never_mutates() {
        let mut receiver = EventReceiver {
            next: u64::MAX - 2,
            ..Default::default()
        };
        let last = EventPacket {
            sequence: u64::MAX - 1,
            payload: vec![9],
        };
        let reserved = EventPacket {
            sequence: u64::MAX,
            payload: vec![],
        };
        assert_eq!(receiver.admit(&last, EVENT_LIMIT), Ok(Receipt::Retained));
        assert_eq!(
            receiver.admit(&reserved, EVENT_LIMIT),
            Err(Error::SequenceExhausted)
        );
        assert_eq!(receiver.receive(&reserved), Err(Error::SequenceExhausted));
        assert_eq!(
            (receiver.next_sequence(), receiver.buffered()),
            (u64::MAX - 2, 1)
        );
        let before = EventPacket {
            sequence: u64::MAX - 2,
            payload: vec![8],
        };
        receiver.admit(&before, EVENT_LIMIT).unwrap();
        let mut output = [None, None];
        assert_eq!(receiver.drain_into(&mut output, 1).delivered, 1);
        assert_eq!(receiver.next_sequence(), u64::MAX - 1);
        assert_eq!(
            receiver
                .drain_ready(1, 1, |s, p| s == last.sequence && p == [9])
                .delivered,
            1
        );
        assert_eq!(receiver.next_sequence(), u64::MAX);
        assert_eq!(receiver.buffered(), 0);
        assert_eq!(receiver.admit(&last, 0), Ok(Receipt::Duplicate));
        assert_eq!(
            receiver.admit(&reserved, EVENT_LIMIT),
            Err(Error::SequenceExhausted)
        );
        assert_eq!(
            receiver.drain_ready(usize::MAX, usize::MAX, |_, _| unreachable!()),
            DeliveryReport::default()
        );
        assert_eq!(receiver.receive(&last), Ok(vec![]));
        assert_eq!(receiver.next_sequence(), u64::MAX);
    }

    #[test]
    fn legacy_final_valid_sequence_is_returned_before_exhaustion() {
        let mut receiver = EventReceiver {
            next: u64::MAX - 1,
            ..Default::default()
        };
        let last = EventPacket {
            sequence: u64::MAX - 1,
            payload: vec![1],
        };
        assert_eq!(receiver.receive(&last), Ok(vec![last]));
        assert_eq!(receiver.next_sequence(), u64::MAX);
        assert_eq!(
            receiver.receive(&EventPacket {
                sequence: u64::MAX,
                payload: vec![2]
            }),
            Err(Error::SequenceExhausted)
        );
        assert_eq!(receiver.buffered(), 0);
    }
}
