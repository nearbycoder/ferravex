//! Bounded, independently instantiated supplied-time datagram router.
//!
//! A two-peer router is a local pair; more peers use the same explicit routing.
//! The caller supplies a nonzero, never-reused `link_id` namespace per router
//! lifetime, and distinct peer IDs. Handles include that namespace and a private
//! generation: reset/disposal rejects stale handles before touching queues. These
//! identities are local routing guards, not wire authentication. Do not reuse a
//! namespace after dropping a router while old handles might still exist.
//!
//! `send` success means **complete local admission**, including packets selected
//! for later configured loss. A full queue returns an error even for such a
//! packet. Return `send(...).is_ok()` to protocol callbacks; intentional loss is
//! observed only by `receive`, never by inventing ACKs. Default settings are a
//! reliable ordered queue. Periodic impairment uses the one-based accepted packet
//! ordinal across all routes: every Nth is lost and/or gets extra reorder delay.
//! Refusals consume no ordinal. No RNG, thread, socket or implicit clock exists.
//!
//! Time is monotonic, exact microsecond `Duration`, 0..=2^53 microseconds, shared
//! across all operations/routes. Equal times are legal. Delay and extra reorder
//! delay are independently <=60 seconds; delivery must fit the time domain.
//! Delivery order is `(due_at, admission ordinal)` per destination, so equal
//! times are exact FIFO even after removals. There is no head-of-line blocking
//! across destinations. Call receive repeatedly with a host work budget; each
//! call removes at most one due packet (including one loss record). Poll empty
//! destinations too if they may hold losses; no background disposal occurs.
//!
//! Limits are aggregate payload bytes and packets, not per-peer. Empty packets
//! reject; maximum is the existing `MAX_PACKET_BYTES` (1200). At most 64 peers,
//! 4096 packets and 4096*1200 logical bytes may be configured; zero queue limits
//! are valid backpressure. Construction reserves storage for all configured
//! packet slots (each admitted packet has an initialized 1200-byte array) and
//! peers with fallible reservation. No send/receive/reset/dispose operation allocates.
//! Physical allocation includes metadata/padding and unused packet-array tails;
//! the byte limit counts only initialized payload prefixes in admitted packets.
//! Queue selection/removal and endpoint checks take bounded linear work.
//!
//! On an existing router, every returned error leaves time, queues, generations,
//! ordinal and caller output unchanged. Success, including an empty receive, advances the time watermark.
//! A short receive output rejects before dequeue; success copies only `len`
//! bytes, preserving its tail. A loss record or empty receive writes nothing.
//! Reset clears all traffic/ordinals and invalidates every live handle, while
//! preserving limits, impairment and monotonic time. Endpoint reset clears both
//! directions for that peer and invalidates its handles; other peers/ordinals
//! remain intact. Dispose permanently removes that peer and its traffic. Drop
//! disposes the whole router. Generation/ordinal exhaustion never wraps.
//!
//! ```
//! use ferravex_net::local_link::{LocalLink, Limits, Impairment};
//! use std::time::Duration;
//! let mut pair = LocalLink::new(42, &[10, 20], Limits::default(), Impairment::default())?;
//! let a = pair.endpoint(10)?;
//! let b = pair.endpoint(20)?;
//! pair.send(a, b, b"whole datagram", Duration::ZERO)?;
//! let mut output = [0xAA; 1200];
//! let delivery = pair.receive(b, Duration::ZERO, &mut output)?.unwrap();
//! assert!(!delivery.lost);
//! assert_eq!(&output[..delivery.len], b"whole datagram");
//! assert_eq!(output[delivery.len], 0xAA);
//! pair.reset(Duration::ZERO)?;
//! assert!(pair.send(a, b, b"stale", Duration::ZERO).is_err());
//! # Ok::<(), ferravex_net::local_link::LinkError>(())
//! ```
use crate::MAX_PACKET_BYTES;
use std::time::Duration;

const MAX_PEERS: usize = 64;
const MAX_PACKETS: usize = 4096;
const MAX_TIME: Duration = Duration::from_micros(1 << 53);

/// Aggregate queue budgets. Defaults: 128 packets and 153600 payload bytes.
/// Both limits apply independently, including to later loss records. Bytes need
/// not equal packets * 1200; a smaller byte cap is useful for mixed packet sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum admitted datagrams, inclusive; 0..=4096.
    pub packets: usize,
    /// Maximum sum of admitted payload lengths, inclusive; 0..=4915200.
    pub bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            packets: 128,
            bytes: 128 * MAX_PACKET_BYTES,
        }
    }
}

/// Deterministic periodic impairment; defaults disable all impairment.
/// `Some(0)` is invalid. Delay is applied to every packet; reorder_delay is
/// additionally applied to every reorder_every-th admitted packet. Loss and
/// reorder selections may overlap. Periods are global to this router, not flows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Impairment {
    pub delay: Duration,
    pub loss_every: Option<u64>,
    pub reorder_every: Option<u64>,
    pub reorder_delay: Duration,
}

/// Explicit peer identity within one live router generation. Use accessors for
/// host address mapping; handles cannot be constructed with a guessed generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Endpoint {
    link_id: u64,
    peer_id: u64,
    generation: u64,
}
impl Endpoint {
    pub fn link_id(self) -> u64 {
        self.link_id
    }
    pub fn peer_id(self) -> u64 {
        self.peer_id
    }
    pub fn generation(self) -> u64 {
        self.generation
    }
}

/// One removed due datagram. `lost=true` is a post-admission impairment record:
/// output was untouched and must not be passed to a protocol driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub from: Endpoint,
    pub to: Endpoint,
    pub len: usize,
    pub admitted_at: Duration,
    pub due_at: Duration,
    pub ordinal: u64,
    pub lost: bool,
}

/// Preflight failures; none mutate the router or caller output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkError {
    InvalidLimits,
    InvalidImpairment,
    InvalidIdentity,
    UnknownPeer,
    StaleEndpoint,
    SameEndpoint,
    EmptyPacket,
    PacketTooLarge,
    PacketCapacity,
    ByteCapacity,
    InvalidTime,
    BackwardsTime,
    DeliveryTimeOverflow,
    OutputTooSmall { required: usize },
    Exhausted,
    AllocationFailed,
}
impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "local datagram link: {self:?}")
    }
}
impl std::error::Error for LinkError {}

struct Peer {
    id: u64,
    generation: u64,
    active: bool,
}
struct Packet {
    delivery: Delivery,
    bytes: [u8; MAX_PACKET_BYTES],
}

/// Bounded datagram storage; no global/shared transport state or implicit I/O.
/// See the module recipe and FPS `local_session` example for driver composition.
pub struct LocalLink {
    link_id: u64,
    peers: Vec<Peer>,
    packets: Vec<Packet>,
    bytes: usize,
    limits: Limits,
    impairment: Impairment,
    ordinal: u64,
    now: Duration,
}
impl LocalLink {
    /// Reserve the complete configured storage. Peer IDs may be zero but must be
    /// distinct; link_id must be nonzero and lifetime-unique in the host.
    pub fn new(
        link_id: u64,
        peers: &[u64],
        limits: Limits,
        impairment: Impairment,
    ) -> Result<Self, LinkError> {
        if link_id == 0
            || peers.len() < 2
            || peers.len() > MAX_PEERS
            || peers
                .iter()
                .enumerate()
                .any(|(i, p)| peers[..i].contains(p))
        {
            return Err(LinkError::InvalidIdentity);
        }
        if limits.packets > MAX_PACKETS || limits.bytes > MAX_PACKETS * MAX_PACKET_BYTES {
            return Err(LinkError::InvalidLimits);
        }
        if impairment.loss_every == Some(0)
            || impairment.reorder_every == Some(0)
            || [impairment.delay, impairment.reorder_delay]
                .iter()
                .any(|d| *d > Duration::from_secs(60) || !d.subsec_nanos().is_multiple_of(1000))
        {
            return Err(LinkError::InvalidImpairment);
        }
        let mut endpoints = Vec::new();
        endpoints
            .try_reserve_exact(peers.len())
            .map_err(|_| LinkError::AllocationFailed)?;
        endpoints.extend(peers.iter().map(|&id| Peer {
            id,
            generation: 1,
            active: true,
        }));
        let mut packets = Vec::new();
        packets
            .try_reserve_exact(limits.packets)
            .map_err(|_| LinkError::AllocationFailed)?;
        Ok(Self {
            link_id,
            peers: endpoints,
            packets,
            bytes: 0,
            limits,
            impairment,
            ordinal: 0,
            now: Duration::ZERO,
        })
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn impairment(&self) -> Impairment {
        self.impairment
    }
    pub fn queued_packets(&self) -> usize {
        self.packets.len()
    }
    pub fn queued_bytes(&self) -> usize {
        self.bytes
    }
    pub fn endpoint(&self, peer_id: u64) -> Result<Endpoint, LinkError> {
        let p = self
            .peers
            .iter()
            .find(|p| p.id == peer_id && p.active)
            .ok_or(LinkError::UnknownPeer)?;
        Ok(Endpoint {
            link_id: self.link_id,
            peer_id,
            generation: p.generation,
        })
    }
    fn peer_index(&self, endpoint: Endpoint) -> Result<usize, LinkError> {
        if endpoint.link_id != self.link_id {
            return Err(LinkError::StaleEndpoint);
        }
        self.peers
            .iter()
            .position(|p| {
                p.active && p.id == endpoint.peer_id && p.generation == endpoint.generation
            })
            .ok_or(LinkError::StaleEndpoint)
    }
    fn check_time(&self, now: Duration) -> Result<(), LinkError> {
        if now > MAX_TIME || !now.subsec_nanos().is_multiple_of(1000) {
            return Err(LinkError::InvalidTime);
        }
        if now < self.now {
            return Err(LinkError::BackwardsTime);
        }
        Ok(())
    }
    /// Admit one complete opaque datagram. Bytes need not be a valid protocol
    /// packet: codecs/drivers retain that responsibility. Errors never consume an
    /// impairment ordinal, capacity or time. Same-peer sends reject.
    pub fn send(
        &mut self,
        from: Endpoint,
        to: Endpoint,
        bytes: &[u8],
        now: Duration,
    ) -> Result<(), LinkError> {
        self.check_time(now)?;
        self.peer_index(from)?;
        self.peer_index(to)?;
        if from == to {
            return Err(LinkError::SameEndpoint);
        }
        if bytes.is_empty() {
            return Err(LinkError::EmptyPacket);
        }
        if bytes.len() > MAX_PACKET_BYTES {
            return Err(LinkError::PacketTooLarge);
        }
        if self.packets.len() == self.limits.packets {
            return Err(LinkError::PacketCapacity);
        }
        if bytes.len() > self.limits.bytes - self.bytes {
            return Err(LinkError::ByteCapacity);
        }
        let ordinal = self.ordinal.checked_add(1).ok_or(LinkError::Exhausted)?;
        let selected = |period: Option<u64>| period.is_some_and(|n| ordinal.is_multiple_of(n));
        let delay = self.impairment.delay
            + if selected(self.impairment.reorder_every) {
                self.impairment.reorder_delay
            } else {
                Duration::ZERO
            };
        let due_at = now
            .checked_add(delay)
            .filter(|t| *t <= MAX_TIME)
            .ok_or(LinkError::DeliveryTimeOverflow)?;
        let mut packet = Packet {
            delivery: Delivery {
                from,
                to,
                len: bytes.len(),
                admitted_at: now,
                due_at,
                ordinal,
                lost: selected(self.impairment.loss_every),
            },
            bytes: [0; MAX_PACKET_BYTES],
        };
        packet.bytes[..bytes.len()].copy_from_slice(bytes);
        self.packets.push(packet);
        self.bytes += bytes.len();
        self.ordinal = ordinal;
        self.now = now;
        Ok(())
    }
    /// Remove at most one due packet for this destination. Short output rejects
    /// without removing even a loss record; empty/lost output stays unchanged.
    /// Work scans <=4096 slots and moves at most one slot (swap removal).
    /// A loss still consumes its admitted packet/byte capacity until removed.
    pub fn receive(
        &mut self,
        to: Endpoint,
        now: Duration,
        output: &mut [u8],
    ) -> Result<Option<Delivery>, LinkError> {
        self.check_time(now)?;
        self.peer_index(to)?;
        let index = self
            .packets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.delivery.to == to && p.delivery.due_at <= now)
            .min_by_key(|(_, p)| (p.delivery.due_at, p.delivery.ordinal))
            .map(|(i, _)| i);
        let Some(index) = index else {
            self.now = now;
            return Ok(None);
        };
        let delivery = self.packets[index].delivery;
        if output.len() < delivery.len {
            return Err(LinkError::OutputTooSmall {
                required: delivery.len,
            });
        }
        let packet = self.packets.swap_remove(index);
        if !delivery.lost {
            output[..delivery.len].copy_from_slice(&packet.bytes[..delivery.len]);
        }
        self.bytes -= delivery.len;
        self.now = now;
        Ok(Some(delivery))
    }
    fn discard_peer(&mut self, peer: Endpoint) {
        self.packets
            .retain(|p| p.delivery.from != peer && p.delivery.to != peer);
        self.bytes = self.packets.iter().map(|p| p.delivery.len).sum();
    }
    /// Invalidate one endpoint and cancel both directions, preserving time and
    /// the global impairment ordinal. The returned handle is the only fresh one.
    ///
    /// ```
    /// use ferravex_net::local_link::{LocalLink, Limits, Impairment};
    /// use std::time::Duration;
    /// let mut link = LocalLink::new(77, &[1, 2], Limits::default(), Impairment::default())?;
    /// let old = link.endpoint(1)?;
    /// let fresh = link.reset_endpoint(old, Duration::ZERO)?;
    /// assert_ne!(old, fresh);
    /// link.dispose(fresh, Duration::ZERO)?;
    /// assert!(link.endpoint(1).is_err());
    /// # Ok::<(), ferravex_net::local_link::LinkError>(())
    /// ```
    pub fn reset_endpoint(&mut self, peer: Endpoint, now: Duration) -> Result<Endpoint, LinkError> {
        self.check_time(now)?;
        let i = self.peer_index(peer)?;
        let generation = self.peers[i]
            .generation
            .checked_add(1)
            .ok_or(LinkError::Exhausted)?;
        self.discard_peer(peer);
        self.peers[i].generation = generation;
        self.now = now;
        self.endpoint(peer.peer_id)
    }
    /// Permanently remove a peer and all its traffic. Repeated disposal rejects.
    pub fn dispose(&mut self, peer: Endpoint, now: Duration) -> Result<(), LinkError> {
        self.check_time(now)?;
        let i = self.peer_index(peer)?;
        self.discard_peer(peer);
        self.peers[i].active = false;
        self.now = now;
        Ok(())
    }
    /// Clear traffic and reset impairment ordinal, invalidating all live handles.
    /// Disposed peers stay disposed; time cannot be rewound or re-originated.
    pub fn reset(&mut self, now: Duration) -> Result<(), LinkError> {
        self.check_time(now)?;
        if self
            .peers
            .iter()
            .any(|p| p.active && p.generation == u64::MAX)
        {
            return Err(LinkError::Exhausted);
        }
        for p in &mut self.peers {
            if p.active {
                p.generation += 1;
            }
        }
        self.packets.clear();
        self.bytes = 0;
        self.ordinal = 0;
        self.now = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhaustion_never_wraps_or_cancels_retained_traffic() {
        let mut link =
            LocalLink::new(1, &[1, 2], Limits::default(), Impairment::default()).unwrap();
        let a = link.endpoint(1).unwrap();
        let b = link.endpoint(2).unwrap();
        link.send(a, b, b"retained", Duration::ZERO).unwrap();
        link.ordinal = u64::MAX;
        assert_eq!(
            link.send(a, b, b"x", Duration::ZERO),
            Err(LinkError::Exhausted)
        );
        link.peers[0].generation = u64::MAX;
        let a = link.endpoint(1).unwrap();
        assert_eq!(
            link.reset_endpoint(a, Duration::ZERO),
            Err(LinkError::Exhausted)
        );
        assert_eq!(link.reset(Duration::ZERO), Err(LinkError::Exhausted));
        assert_eq!(link.queued_bytes(), 8);
    }
}
