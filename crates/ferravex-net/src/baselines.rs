//! Bounded, separate sent and installed snapshot histories.
//!
//! An ACK authorizes only the **exact bytes still retained** for that tick. This
//! proves local sent-byte retention, not remote application acceptance: callers
//! must authenticate the packet/session and define an ACK to mean the receiver
//! validated, accepted and installed those bytes. A transport ACK is insufficient.
//! Receiver application is separate from install, so unacceptable reconstructed
//! snapshots never become baselines automatically.
//!
//! Both stores reserve metadata and a byte arena at construction. Thereafter
//! retain, ACK, select, apply and reset allocate nothing. Limits bound snapshot
//! count and the sum of retained byte lengths; empty snapshots count too.
//! Allocator overhead is excluded. Eviction removes an oldest insertion prefix,
//! including an acknowledged baseline when necessary; no pinned copy bypasses
//! the byte limit. Compaction costs O(retained bytes + snapshot count). Rejected
//! operations preserve the store and output.
//!
//! Ticks advance by [`crate::sequence_newer`], including `u32::MAX -> 0`.
//! Identical retained repeats are idempotent without refreshing eviction order;
//! conflicting repeats, evicted repeats, backward/half-range ticks and a retained
//! window spanning half the tick space are rejected. Internal nonwrapping u64
//! insertion generations order ACKs even across tick wrap. Generation exhaustion
//! requires reset. No local generation is transmitted or authenticates packets.
//! Call `reset` on both stores on reconnect/session change and validate the
//! external session epoch **before every call**, including ACKs after reset.
//! Reset cannot distinguish a delayed old-session ACK with a reused tick.
//! Tick-only ACKs also cannot identify a prior revolution in the same session:
//! callers must expire packets within half the tick space, or rotate the session
//! before tick reuse. Local generations only order known retained insertions.
//!
//! This runnable first-party recipe uses real snapshot/ACK/delta codecs. The
//! `journey400_229` integration test also covers lost/evicted baselines, full
//! recovery and membership length changes. Outer packet encoding still allocates.
//!
//! ```
//! use ferravex_net::{
//!     baselines::{AckOutcome, BaselineLimits, Selection, SenderBaselines, ReceiverBaselines},
//!     delta::Delta, ClientExtension, ClientPacket, ServerExtension, ServerPacket,
//!     Snapshot, MAX_PACKET_BYTES, decode_client, decode_server, encode_client, encode_server,
//! };
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let limits = BaselineLimits { max_snapshots: 32, max_bytes: 32 * MAX_PACKET_BYTES };
//!     let mut sent = SenderBaselines::new(limits)?;
//!     let mut received = ReceiverBaselines::new(limits)?;
//!     let first = Snapshot { session: 7, tick: 40, ..Snapshot::default() };
//!     let full = encode_server(&ServerPacket::Snapshot(first.clone()))?;
//!     // Deliver successfully, then record the exact full representation sent.
//!     sent.record_sent(first.tick, &full)?;
//!     assert_eq!(decode_server(&full)?, ServerPacket::Snapshot(first.clone()));
//!     received.install(first.tick, &full)?; // Consumer accepts this snapshot.
//!     let ack = encode_client(&ClientPacket::Extension(ClientExtension::SnapshotAck {
//!         session: 7, tick: Some(40),
//!     }))?;
//!     let ClientPacket::Extension(ClientExtension::SnapshotAck { session: 7, tick: Some(tick) })
//!         = decode_client(&ack)? else { panic!("wrong session/ACK") };
//!     assert_eq!(sent.acknowledge(tick), AckOutcome::Accepted);
//!     let next = Snapshot { tick: 41, ..first };
//!     let full = encode_server(&ServerPacket::Snapshot(next.clone()))?;
//!     let mut delta = Delta { base_tick: 0, tick: 0, bytes: Vec::with_capacity(MAX_PACKET_BYTES) };
//!     assert!(matches!(sent.select_into(41, &full, &mut delta, 24, MAX_PACKET_BYTES),
//!         Selection::Delta { base_tick: 40, .. }));
//!     let wire = encode_server(&ServerPacket::Extension(ServerExtension::SnapshotDelta {
//!         session: 7, delta,
//!     }))?;
//!     sent.record_sent(41, &full)?; // Retain reconstructed bytes, not delta bytes.
//!     let ServerPacket::Extension(ServerExtension::SnapshotDelta { session: 7, delta })
//!         = decode_server(&wire)? else { panic!("wrong session/delta") };
//!     let mut output = [0; MAX_PACKET_BYTES];
//!     let n = received.apply_into(&delta, &mut output)?;
//!     let ServerPacket::Snapshot(snapshot) = decode_server(&output[..n])?
//!         else { panic!("not a snapshot") };
//!     assert_eq!((snapshot.session, snapshot.tick), (7, delta.tick));
//!     assert_eq!(snapshot, next); // Validate/accept before installation or ACK.
//!     received.install(snapshot.tick, &output[..n])?;
//!     let ack = encode_client(&ClientPacket::Extension(ClientExtension::SnapshotAck {
//!         session: snapshot.session, tick: Some(snapshot.tick),
//!     }))?;
//!     let ClientPacket::Extension(ClientExtension::SnapshotAck { session: 7, tick: Some(tick) })
//!         = decode_client(&ack)? else { panic!("wrong session/ACK") };
//!     assert_eq!(sent.acknowledge(tick), AckOutcome::Accepted);
//!     assert_eq!(sent.baseline(), Some((41, &output[..n])));
//!     sent.reset(); received.reset();
//!     assert_eq!(sent.acknowledge(41), AckOutcome::Unknown);
//!     Ok(())
//! }
//! ```

use crate::{
    delta::{Delta, DeltaError},
    sequence_newer,
};

/// Explicit per-peer storage budget. Zero bytes permits only empty snapshots;
/// zero snapshots is invalid. See the module recipe for construction and use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaselineLimits {
    pub max_snapshots: usize,
    pub max_bytes: usize,
}

/// Retention/construction failure. Every error leaves an existing store intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineError {
    ZeroSnapshotCapacity,
    AllocationFailed,
    SnapshotTooLarge { required: usize, max: usize },
    ConflictingTick { tick: u32 },
    TickNotNewer { tick: u32, latest: u32 },
    TickWindow { tick: u32, oldest: u32 },
    GenerationExhausted,
}
impl std::fmt::Display for BaselineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "baseline retention failed: {self:?}")
    }
}
impl std::error::Error for BaselineError {}

/// Successful retention. A duplicate neither evicts nor advances generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainOutcome {
    Stored { evicted: usize },
    Duplicate,
}

/// ACK decisions require exact retained membership before checking generation.
/// Unknown (including evicted) and stale ACKs never change authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AckOutcome {
    Accepted,
    AlreadyAcknowledged,
    Stale,
    Unknown,
}

/// Why selection recommends the caller's full snapshot. This does not validate
/// that the full packet fits its protocol, transport, or retention budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullReason {
    NoAcknowledgement,
    BaselineEvicted { tick: u32 },
    TickNotNewer { tick: u32, baseline: u32 },
    LengthChanged { baseline: usize, next: usize },
    Delta(DeltaError),
    PacketLimit,
    NotSmaller,
}

/// Only `Delta` means the supplied reusable delta was replaced. Full fallback
/// leaves it unchanged; branch on this result before encoding that delta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Full(FullReason),
    Delta { base_tick: u32, packet_bytes: usize },
}

/// Application failure preserves the entire caller output and installed store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyError {
    MissingBaseline { tick: u32 },
    TickNotNewer { tick: u32, previous: u32 },
    Delta(DeltaError),
}
impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "baseline application failed: {self:?}")
    }
}
impl std::error::Error for ApplyError {}

#[derive(Debug)]
struct Entry {
    tick: u32,
    generation: u64,
    start: usize,
    len: usize,
}

#[derive(Debug)]
struct Store {
    limits: BaselineLimits,
    entries: Vec<Entry>,
    bytes: Vec<u8>,
    generation: u64,
}
impl Store {
    fn new(limits: BaselineLimits) -> Result<Self, BaselineError> {
        if limits.max_snapshots == 0 {
            return Err(BaselineError::ZeroSnapshotCapacity);
        }
        let mut entries = Vec::new();
        let mut bytes = Vec::new();
        entries
            .try_reserve_exact(limits.max_snapshots)
            .map_err(|_| BaselineError::AllocationFailed)?;
        bytes
            .try_reserve_exact(limits.max_bytes)
            .map_err(|_| BaselineError::AllocationFailed)?;
        Ok(Self {
            limits,
            entries,
            bytes,
            generation: 0,
        })
    }

    fn entry(&self, tick: u32) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.tick == tick)
    }
    fn entry_bytes(&self, entry: &Entry) -> &[u8] {
        &self.bytes[entry.start..entry.start + entry.len]
    }
    fn get(&self, tick: u32) -> Option<&[u8]> {
        self.entry(tick).map(|entry| self.entry_bytes(entry))
    }
    fn latest_tick(&self) -> Option<u32> {
        self.entries.last().map(|entry| entry.tick)
    }

    fn retain(&mut self, tick: u32, bytes: &[u8]) -> Result<RetainOutcome, BaselineError> {
        if let Some(old) = self.get(tick) {
            return if old == bytes {
                Ok(RetainOutcome::Duplicate)
            } else {
                Err(BaselineError::ConflictingTick { tick })
            };
        }
        if bytes.len() > self.limits.max_bytes {
            return Err(BaselineError::SnapshotTooLarge {
                required: bytes.len(),
                max: self.limits.max_bytes,
            });
        }
        if let Some(latest) = self.latest_tick()
            && !sequence_newer(tick, latest)
        {
            return Err(BaselineError::TickNotNewer { tick, latest });
        }
        if let Some(oldest) = self.entries.first().map(|entry| entry.tick)
            && !sequence_newer(tick, oldest)
        {
            return Err(BaselineError::TickWindow { tick, oldest });
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(BaselineError::GenerationExhausted)?;
        // Complete preflight before eviction; subtraction avoids size overflow.
        let mut evicted = 0;
        let mut removed_bytes = 0;
        while self.entries.len() - evicted >= self.limits.max_snapshots
            || self.bytes.len() - removed_bytes > self.limits.max_bytes - bytes.len()
        {
            removed_bytes += self.entries[evicted].len;
            evicted += 1;
        }
        if evicted != 0 {
            self.entries.drain(..evicted);
            self.bytes.copy_within(removed_bytes.., 0);
            self.bytes.truncate(self.bytes.len() - removed_bytes);
            for entry in &mut self.entries {
                entry.start -= removed_bytes;
            }
        }
        self.entries.push(Entry {
            tick,
            generation,
            start: self.bytes.len(),
            len: bytes.len(),
        });
        self.bytes.extend_from_slice(bytes);
        self.generation = generation;
        Ok(RetainOutcome::Stored { evicted })
    }

    fn reset(&mut self) {
        self.entries.clear();
        self.bytes.clear();
        self.generation = 0;
    }
}

/// Exact sent bytes with ACK authorization, separate from installed state.
/// See the module recipe for the required send/install/ACK sequence.
#[derive(Debug)]
pub struct SenderBaselines {
    store: Store,
    acknowledged: Option<(u32, u64)>,
}
impl SenderBaselines {
    /// Fallibly preallocate both budgets. No implicit FPS/game defaults.
    pub fn new(limits: BaselineLimits) -> Result<Self, BaselineError> {
        Ok(Self {
            store: Store::new(limits)?,
            acknowledged: None,
        })
    }

    /// Record full reconstructed bytes only after the caller's send succeeds.
    /// Failed sends must not be recorded. This does not send or authorize bytes.
    /// Preflight snapshot size when guaranteed retention before send is needed;
    /// an over-budget successful send cannot subsequently be ACKed here.
    pub fn record_sent(&mut self, tick: u32, bytes: &[u8]) -> Result<RetainOutcome, BaselineError> {
        self.store.retain(tick, bytes)
    }

    /// Authenticate the session externally first. Exact retained membership is
    /// mandatory; a newer unknown tick never authorizes bytes. An older retained
    /// generation cannot roll authorization back, even after eviction of the
    /// acknowledged generation. ACKs do not refresh eviction order.
    pub fn acknowledge(&mut self, tick: u32) -> AckOutcome {
        let Some(entry) = self.store.entry(tick) else {
            return AckOutcome::Unknown;
        };
        if let Some((_, generation)) = self.acknowledged {
            if entry.generation < generation {
                return AckOutcome::Stale;
            }
            if entry.generation == generation {
                return AckOutcome::AlreadyAcknowledged;
            }
        }
        self.acknowledged = Some((tick, entry.generation));
        AckOutcome::Accepted
    }

    /// Currently authorized retained bytes. Eviction returns None; the ACK
    /// watermark alone is never sufficient to select a delta.
    pub fn baseline(&self) -> Option<(u32, &[u8])> {
        let (tick, generation) = self.acknowledged?;
        let entry = self.store.entry(tick)?;
        (entry.generation == generation).then(|| (tick, self.store.entry_bytes(entry)))
    }

    /// Select against exact ACKed retained bytes using checked `Delta`.
    /// `envelope_bytes` is the entire outer overhead (24 for current v5 packets),
    /// and `packet_limit` is the caller's transport/protocol bound. Delta must
    /// fit that bound and be strictly smaller than `full.len()`. Overflow,
    /// incompatible lengths, codec limits and insufficient Delta capacity select
    /// full without mutation. No encoding, retention or ACK is implicit. Outer
    /// full-packet validation, tick acceptance and encoding failures remain
    /// caller policy; a full outcome does not authorize sending a stale tick.
    pub fn select_into(
        &self,
        tick: u32,
        full: &[u8],
        delta: &mut Delta,
        envelope_bytes: usize,
        packet_limit: usize,
    ) -> Selection {
        let Some((ack_tick, _)) = self.acknowledged else {
            return Selection::Full(FullReason::NoAcknowledgement);
        };
        let Some((base_tick, base)) = self.baseline() else {
            return Selection::Full(FullReason::BaselineEvicted { tick: ack_tick });
        };
        if !sequence_newer(tick, base_tick) {
            return Selection::Full(FullReason::TickNotNewer {
                tick,
                baseline: base_tick,
            });
        }
        if base.len() != full.len() {
            return Selection::Full(FullReason::LengthChanged {
                baseline: base.len(),
                next: full.len(),
            });
        }
        let encoded_len = match Delta::encoded_len(base, full) {
            Ok(len) => len,
            Err(error) => return Selection::Full(FullReason::Delta(error)),
        };
        let Some(packet_bytes) = encoded_len
            .checked_add(envelope_bytes)
            .filter(|&len| len <= packet_limit)
        else {
            return Selection::Full(FullReason::PacketLimit);
        };
        if packet_bytes >= full.len() {
            return Selection::Full(FullReason::NotSmaller);
        }
        match delta.between_into(base_tick, base, tick, full) {
            Ok(()) => Selection::Delta {
                base_tick,
                packet_bytes,
            },
            Err(error) => Selection::Full(FullReason::Delta(error)),
        }
    }

    /// Exact retained sent bytes; lookup does not authorize them.
    pub fn get(&self, tick: u32) -> Option<&[u8]> {
        self.store.get(tick)
    }
    /// Current snapshot count, including empty snapshots.
    pub fn len(&self) -> usize {
        self.store.entries.len()
    }
    /// Whether there are no retained snapshots.
    pub fn is_empty(&self) -> bool {
        self.store.entries.is_empty()
    }
    /// Sum of retained byte lengths, without a separate acknowledged copy.
    pub fn retained_bytes(&self) -> usize {
        self.store.bytes.len()
    }
    /// Last retained tick; duplicates do not change it.
    pub fn latest_tick(&self) -> Option<u32> {
        self.store.latest_tick()
    }
    /// Clear bytes, ACK watermark, ticks and generations, retaining allocations.
    /// Suitable for authenticated full recovery or session reset. See the module
    /// session-epoch requirement; reset does not filter delayed packets.
    pub fn reset(&mut self) {
        self.store.reset();
        self.acknowledged = None;
    }
}

/// Receiver-only installed bytes. Applying into scratch never installs or ACKs
/// the result; the caller must decode, validate and accept it first.
#[derive(Debug)]
pub struct ReceiverBaselines {
    store: Store,
}
impl ReceiverBaselines {
    /// Fallibly preallocate count and byte budgets; see the module recipe.
    pub fn new(limits: BaselineLimits) -> Result<Self, BaselineError> {
        Ok(Self {
            store: Store::new(limits)?,
        })
    }
    /// Retain bytes after application validation/acceptance, before issuing the
    /// snapshot ACK. On failure, do not ACK or publish the snapshot. This method
    /// does not parse bytes or check external session/membership/tick fields.
    pub fn install(&mut self, tick: u32, bytes: &[u8]) -> Result<RetainOutcome, BaselineError> {
        self.store.retain(tick, bytes)
    }
    /// Apply against the exact installed base tick into caller scratch. Reject
    /// nonadvancing result ticks, missing baselines and invalid deltas before
    /// writes. Success preserves the output tail and installed history. Missing
    /// or invalid baselines need caller-controlled full recovery; preserve request
    /// rate limits. Nonadvancing packets can simply be ignored. The codec cannot
    /// detect same-length baseline content from another session: external session
    /// authentication and immutable exact-byte installation are required.
    pub fn apply_into(&self, delta: &Delta, destination: &mut [u8]) -> Result<usize, ApplyError> {
        if let Some(previous) = self.latest_tick()
            && !sequence_newer(delta.tick, previous)
        {
            return Err(ApplyError::TickNotNewer {
                tick: delta.tick,
                previous,
            });
        }
        let base = self
            .get(delta.base_tick)
            .ok_or(ApplyError::MissingBaseline {
                tick: delta.base_tick,
            })?;
        if !sequence_newer(delta.tick, delta.base_tick) {
            return Err(ApplyError::TickNotNewer {
                tick: delta.tick,
                previous: delta.base_tick,
            });
        }
        delta
            .apply_into(delta.base_tick, base, destination)
            .map_err(ApplyError::Delta)
    }
    /// Exact installed bytes available for reconstruction.
    pub fn get(&self, tick: u32) -> Option<&[u8]> {
        self.store.get(tick)
    }
    /// Current snapshot count, including empty snapshots.
    pub fn len(&self) -> usize {
        self.store.entries.len()
    }
    /// Whether there are no installed snapshots.
    pub fn is_empty(&self) -> bool {
        self.store.entries.is_empty()
    }
    /// Sum of retained installed byte lengths.
    pub fn retained_bytes(&self) -> usize {
        self.store.bytes.len()
    }
    /// Latest installed tick, distinct from a merely applied scratch result.
    pub fn latest_tick(&self) -> Option<u32> {
        self.store.latest_tick()
    }
    /// Clear installed bytes, ticks and generations, keeping allocations.
    /// External session checks remain mandatory after reset.
    pub fn reset(&mut self) {
        self.store.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_exhaustion_preserves_bytes_and_authorization_until_reset() {
        let mut sender = SenderBaselines::new(BaselineLimits {
            max_snapshots: 1,
            max_bytes: 1,
        })
        .unwrap();
        sender.record_sent(7, &[9]).unwrap();
        assert_eq!(sender.acknowledge(7), AckOutcome::Accepted);
        sender.store.generation = u64::MAX;
        assert_eq!(
            sender.record_sent(8, &[8]),
            Err(BaselineError::GenerationExhausted)
        );
        assert_eq!(sender.baseline(), Some((7, &[9][..])));
        assert_eq!(sender.acknowledge(8), AckOutcome::Unknown);
        sender.reset();
        assert_eq!(
            sender.record_sent(8, &[8]),
            Ok(RetainOutcome::Stored { evicted: 0 })
        );
    }
}
