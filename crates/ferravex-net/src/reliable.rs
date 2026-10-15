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
            .filter(|p| {
                p.last_sent
                    .is_none_or(|last| now.saturating_sub(last) >= RETRY_MS)
            })
            .take(budget.min(EVENT_LIMIT))
            .map(|p| {
                p.last_sent = Some(now);
                p.packet.clone()
            })
            .collect()
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
#[derive(Default)]
pub struct EventReceiver {
    next: u64,
    buffered: BTreeMap<u64, Vec<u8>>,
}
impl EventReceiver {
    /// ACK only on success, including duplicates. Delivery is strictly once and ordered.
    /// The receive window bounds memory even when event zero never arrives.
    pub fn receive(&mut self, packet: &EventPacket) -> Result<Vec<EventPacket>, Error> {
        if packet.payload.len() > EVENT_BYTES {
            return Err(Error::Oversized);
        }
        if packet.sequence < self.next {
            return Ok(Vec::new());
        }
        if packet.sequence - self.next >= EVENT_LIMIT as u64 {
            return Err(Error::OutsideWindow);
        }
        self.buffered
            .entry(packet.sequence)
            .or_insert_with(|| packet.payload.clone());
        let mut delivered = Vec::new();
        while let Some(payload) = self.buffered.remove(&self.next) {
            let sequence = self.next;
            self.next = self.next.checked_add(1).ok_or(Error::SequenceExhausted)?;
            delivered.push(EventPacket { sequence, payload });
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
}
