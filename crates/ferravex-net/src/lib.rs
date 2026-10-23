//! Bounded, versioned network protocol shared by native and browser game clients.
//!
//! The protocol never accepts client positions or client-selected time steps.
//! Native UDP is intentionally a thin primitive:
//! accounts/encryption and NAT traversal remain outside this prototype. The
//! first-party WebSocket gateway carries browser datagrams to the same authority.
pub mod baselines;
pub mod clock;
pub mod delta;
pub mod history;
pub mod local_link;
pub mod reliable;
pub mod time;
#[cfg(not(target_arch = "wasm32"))]
pub mod websocket;
mod wire_v4;
pub use wire_v4::{Client as ClientExtension, Server as ServerExtension, TimedInput};

use std::collections::VecDeque;
use std::fmt;

pub const MAX_PACKET_BYTES: usize = 1200;
pub const MAX_PLAYERS: usize = 16;
pub const MAX_INPUTS_PER_PACKET: usize = 6;
pub const MAX_QUEUED_INPUTS: usize = 64;
pub const BUTTON_JUMP: u8 = 1;
pub const BUTTON_FIRE: u8 = 2;
pub const BUTTON_RELOAD: u8 = 4;
pub const MAX_PITCH: f32 = 1.55;
const MAGIC: &[u8; 4] = b"FVRX";
/// Version 4 adds timed inputs, clock exchanges, snapshot ACK/deltas and commands.
// Version 5 preserves the v4 payloads and gates the retuned prediction rules.
pub const PROTOCOL_VERSION: u8 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputFrame {
    pub sequence: u32,
    /// Strafe right in [-1, 1].
    pub move_x: f32,
    /// Move forward in [-1, 1]; yaw zero faces negative Z.
    pub move_z: f32,
    pub yaw: f32,
    /// Positive pitch looks up.
    pub pitch: f32,
    pub buttons: u8,
}
impl InputFrame {
    pub fn sanitized(mut self) -> Self {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        self.move_x = finite(self.move_x).clamp(-1.0, 1.0);
        self.move_z = finite(self.move_z).clamp(-1.0, 1.0);
        self.yaw = (finite(self.yaw) + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.pitch = finite(self.pitch).clamp(-MAX_PITCH, MAX_PITCH);
        self.buttons &= BUTTON_JUMP | BUTTON_FIRE | BUTTON_RELOAD;
        self
    }
    pub fn is_valid(self) -> bool {
        self.move_x.is_finite()
            && self.move_x.abs() <= 1.0
            && self.move_z.is_finite()
            && self.move_z.abs() <= 1.0
            && self.yaw.is_finite()
            && self.yaw.abs() <= std::f32::consts::PI
            && self.pitch.is_finite()
            && self.pitch.abs() <= MAX_PITCH
            && self.buttons & !(BUTTON_JUMP | BUTTON_FIRE | BUTTON_RELOAD) == 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientPacket {
    Extension(ClientExtension),
    EventAck {
        session: u64,
        sequence: u64,
    },
    Hello {
        nonce: u64,
    },
    Input {
        session: u64,
        inputs: Vec<InputFrame>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum ServerPacket {
    Extension(ServerExtension),
    ReliableEvent {
        session: u64,
        event: reliable::EventPacket,
    },
    Welcome {
        nonce: u64,
        session: u64,
        player_id: u8,
        tick: u32,
    },
    Snapshot(Snapshot),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// Session cookie prevents delayed packets from a prior connection being accepted.
    pub session: u64,
    pub tick: u32,
    pub ack_sequence: u32,
    pub ack_valid: bool,
    /// Low 32 bits of the authority life generation for each player slot.
    pub epochs: [u32; MAX_PLAYERS],
    pub players: Vec<PlayerSnapshot>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerSnapshot {
    pub id: u8,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub health: u8,
    pub ammo: u8,
    pub grounded: bool,
    pub jump_coyote: f32,
    pub jump_buffer: f32,
    pub jump_held: bool,
    pub fire_cooldown: u16,
    pub reload_ticks: u16,
    pub respawn_ticks: u16,
}
impl PlayerSnapshot {
    fn is_valid(self) -> bool {
        (self.id as usize) < MAX_PLAYERS
            && self
                .position
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 4096.0)
            && self
                .velocity
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 256.0)
            && self.yaw.is_finite()
            && self.yaw.abs() <= std::f32::consts::PI
            && self.pitch.is_finite()
            && self.pitch.abs() <= MAX_PITCH
            && self.jump_coyote.is_finite()
            && (0. ..=0.5).contains(&self.jump_coyote)
            && self.jump_buffer.is_finite()
            && (0. ..=0.5).contains(&self.jump_buffer)
            && self.health <= 100
            && self.ammo <= 30
            && self.fire_cooldown <= 4096
            && self.reload_ticks <= 4096
            && self.respawn_ticks <= 4096
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    TooLarge,
    Truncated,
    Header,
    Version,
    Kind,
    Invalid,
    Trailing,
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Ferravex packet: {self:?}")
    }
}
impl std::error::Error for ProtocolError {}

/// RFC-1982-style comparison. Exactly half a sequence space is ambiguous and rejected.
pub fn sequence_newer(candidate: u32, previous: u32) -> bool {
    let delta = candidate.wrapping_sub(previous);
    delta != 0 && delta < (1 << 31)
}

/// Reason a checked input admission was rejected.
///
/// Validation precedes capacity, which precedes sequence ordering. A rejection
/// changes only its metric, never pending controls or the sequence watermark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAdmissionError {
    /// The embedded [`InputFrame`] fails [`InputFrame::is_valid`].
    InvalidInput,
    /// The queue already contains [`MAX_QUEUED_INPUTS`] entries.
    Full,
    /// The sequence equals the newest accepted sequence, even after removal.
    DuplicateNewest,
    /// The sequence is older, or exactly half a sequence space away.
    /// Repeats of earlier accepted sequences also fall in this category.
    OutOfOrder,
}
impl fmt::Display for InputAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "input queue admission rejected: {self:?}")
    }
}
impl std::error::Error for InputAdmissionError {}

/// Queue-local activity since creation or the last [`InputQueue::reset_metrics`].
///
/// Every field saturates independently at [`u64::MAX`]. These count operations,
/// not packets, ticks, simulated actions or remote delivery. Both legacy bool
/// and checked admissions count exactly once. Admission never evicts an input.
/// Coalesced and explicitly discarded entries are separate from rejections and
/// from entries returned to the caller. Before saturation, absent a metrics
/// reset with pending entries, accepted = pending + returned + coalesced + discarded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputQueueMetrics {
    /// Successfully appended inputs.
    pub accepted_inputs: u64,
    /// Admissions rejected for invalid embedded controls.
    pub rejected_invalid: u64,
    /// Admissions rejected for capacity, including stale sequences at capacity.
    pub rejected_full: u64,
    /// Admissions rejected for equality with the newest accepted sequence.
    pub rejected_duplicate_newest: u64,
    /// Admissions rejected for older or half-range ambiguous sequences.
    pub rejected_out_of_order: u64,
    /// Entries returned by pop operations; one per nonempty coalescing call.
    /// Returning an entry does not establish that simulation consumed it.
    pub returned_inputs: u64,
    /// Older entries folded into a returned newest entry by coalescing.
    /// Their action bits and the first recovered fire aim/time can survive.
    pub coalesced_inputs: u64,
    /// Pending entries dropped by clear_pending or reset_session, without return.
    pub discarded_inputs: u64,
}

/// Deduplicates input redundancy and bounds per-client simulation backlog.
///
/// Checked admission preserves the legacy bool methods' ordering and bounds.
/// All [`TimedInput::shot_tick`] values (including a tick without a fire bit)
/// remain accepted when the embedded frame is valid; authority rewind policy
/// belongs to the consumer, not this queue. No protocol bytes are changed.
///
/// Receive controls independently of rendering or local menus. Pop only for an
/// actual authoritative tick; a local overlay must not suspend network/replay
/// progression. Queue metrics describe removal, not executed simulation work.
///
/// This supplied-input recipe recovers an older fire press while using newest
/// movement. Hand the returned command and shot time to the authority's fixed
/// tick, then derive feedback from its accepted game events.
///
/// ```
/// use ferravex_net::{
///     BUTTON_FIRE, InputAdmissionError, InputFrame, InputQueue,
///     InputQueueMetrics, TimedInput,
/// };
///
/// let mut queue = InputQueue::default();
/// queue.push_timed_checked(TimedInput {
///     input: InputFrame {
///         sequence: 40, yaw: 1.0, pitch: 0.25, buttons: BUTTON_FIRE,
///         ..InputFrame::default()
///     },
///     shot_tick: Some(120),
/// }).unwrap();
/// let movement = InputFrame { sequence: 41, move_x: 1.0, ..InputFrame::default() };
/// queue.push_checked(movement).unwrap();
/// assert_eq!(queue.push_checked(movement), Err(InputAdmissionError::DuplicateNewest));
/// // At the next actual authority tick:
/// let command = queue.pop_latest_timed().unwrap();
/// assert_eq!(command.input.sequence, 41);
/// assert_eq!(command.input.move_x, 1.0);
/// assert_eq!((command.input.yaw, command.input.pitch), (1.0, 0.25));
/// assert_eq!(command.input.buttons, BUTTON_FIRE);
/// assert_eq!(command.shot_tick, Some(120));
/// let metrics = queue.metrics(); // Read-only copy for host diagnostics.
/// assert_eq!(metrics.returned_inputs, 1);
/// assert_eq!(metrics.coalesced_inputs, 1);
/// assert_eq!(metrics.rejected_duplicate_newest, 1);
///
/// // A round reset discards pending controls but keeps replay protection.
/// assert!(queue.push(InputFrame { sequence: 42, ..movement }));
/// queue.clear_pending();
/// assert_eq!(queue.metrics().discarded_inputs, 1);
/// assert_eq!(queue.push_checked(movement), Err(InputAdmissionError::OutOfOrder));
/// let previous_window = queue.reset_metrics();
/// assert_eq!(previous_window.accepted_inputs, 3);
/// assert_eq!(queue.metrics(), InputQueueMetrics::default());
/// assert!(!queue.push(InputFrame { sequence: 42, ..movement }));
/// // Only after the host establishes a new authenticated session:
/// queue.reset_session(); // Clears pending and watermark; retains metrics/storage.
/// assert!(queue.push(InputFrame::default()));
/// ```
#[derive(Debug, Default)]
pub struct InputQueue {
    queue: VecDeque<TimedInput>,
    newest: Option<u32>,
    metrics: InputQueueMetrics,
}
impl InputQueue {
    /// Legacy admission; true exactly when [`Self::push_checked`] succeeds.
    pub fn push(&mut self, input: InputFrame) -> bool {
        self.push_checked(input).is_ok()
    }
    /// Legacy timed admission; counts once through [`Self::push_timed_checked`].
    pub fn push_timed(&mut self, timed: TimedInput) -> bool {
        self.push_timed_checked(timed).is_ok()
    }
    /// Append untimed controls or report the first rejection reason.
    ///
    /// Equivalent to [`Self::push_timed_checked`] with no shot tick. See the
    /// [`InputQueue`] recipe for admission, coalescing and reset scopes.
    pub fn push_checked(&mut self, input: InputFrame) -> Result<(), InputAdmissionError> {
        self.push_timed_checked(TimedInput {
            input,
            shot_tick: None,
        })
    }
    /// Append timed controls, validating input, capacity, then sequence ordering.
    ///
    /// Failure preserves all pending values and the watermark, and increments
    /// exactly one saturating rejection counter. Only the embedded frame is
    /// validated: shot_tick is opaque, as in the existing wire protocol.
    /// Success appends without eviction and advances the accepted watermark.
    pub fn push_timed_checked(&mut self, timed: TimedInput) -> Result<(), InputAdmissionError> {
        let input = timed.input;
        let rejection = if !input.is_valid() {
            Some((
                InputAdmissionError::InvalidInput,
                &mut self.metrics.rejected_invalid,
            ))
        } else if self.queue.len() >= MAX_QUEUED_INPUTS {
            Some((InputAdmissionError::Full, &mut self.metrics.rejected_full))
        } else if self.newest == Some(input.sequence) {
            Some((
                InputAdmissionError::DuplicateNewest,
                &mut self.metrics.rejected_duplicate_newest,
            ))
        } else if self
            .newest
            .is_some_and(|old| !sequence_newer(input.sequence, old))
        {
            Some((
                InputAdmissionError::OutOfOrder,
                &mut self.metrics.rejected_out_of_order,
            ))
        } else {
            None
        };
        if let Some((reason, counter)) = rejection {
            *counter = counter.saturating_add(1);
            return Err(reason);
        }
        self.newest = Some(input.sequence);
        self.queue.push_back(timed);
        self.metrics.accepted_inputs = self.metrics.accepted_inputs.saturating_add(1);
        Ok(())
    }
    /// Read a copy of queue-local counters without changing any state.
    pub fn metrics(&self) -> InputQueueMetrics {
        self.metrics
    }
    /// Begin a new metrics window and return the previous counters.
    ///
    /// Pending inputs, allocated storage and the accepted sequence watermark
    /// are retained. Later removals can count inputs admitted before this reset.
    pub fn reset_metrics(&mut self) -> InputQueueMetrics {
        std::mem::take(&mut self.metrics)
    }
    /// Discard queued controls while retaining the sequence watermark and metrics.
    ///
    /// Increments discarded_inputs by the number removed; retains storage.
    /// Suitable for an authority round reset, never a local menu pause.
    pub fn clear_pending(&mut self) {
        self.metrics.discarded_inputs = self
            .metrics
            .discarded_inputs
            .saturating_add(self.queue.len() as u64);
        self.queue.clear();
    }
    /// Discard pending controls and forget the accepted sequence watermark.
    ///
    /// Retains allocated storage and metrics (including these discards). Call
    /// reset_metrics separately if a new reporting window is wanted. Use only
    /// when the host establishes a new session; using this for a round/menu
    /// would allow delayed controls from the same session to re-enter.
    pub fn reset_session(&mut self) {
        self.clear_pending();
        self.newest = None;
    }
    /// Return the oldest input without its shot time, counting one returned entry.
    pub fn pop(&mut self) -> Option<InputFrame> {
        let timed = self.queue.pop_front()?;
        self.metrics.returned_inputs = self.metrics.returned_inputs.saturating_add(1);
        Some(timed.input)
    }
    /// Coalesce a burst to its newest movement/look sample without losing a
    /// queued action press. The authority still simulates exactly one fixed step.
    /// This prevents redundant commands recovered after loss from adding permanent
    /// control latency. Previously consumed commands cannot re-enter the queue.
    pub fn pop_latest(&mut self) -> Option<InputFrame> {
        self.pop_latest_timed().map(|v| v.input)
    }
    /// A recovered older quick-fire press retains its original aim and shot time.
    ///
    /// Returns one entry and counts all older entries as coalesced. The newest
    /// fire sample wins if present; otherwise the oldest queued fire sample wins.
    /// Empty pops change no counters. Removal never resets the watermark.
    pub fn pop_latest_timed(&mut self) -> Option<TimedInput> {
        let mut latest = self.queue.pop_back()?;
        self.metrics.returned_inputs = self.metrics.returned_inputs.saturating_add(1);
        self.metrics.coalesced_inputs = self
            .metrics
            .coalesced_inputs
            .saturating_add(self.queue.len() as u64);
        for older in self.queue.drain(..) {
            if latest.input.buttons & BUTTON_FIRE == 0 && older.input.buttons & BUTTON_FIRE != 0 {
                latest.input.yaw = older.input.yaw;
                latest.input.pitch = older.input.pitch;
                latest.shot_tick = older.shot_tick;
            }
            latest.input.buttons |= older.input.buttons;
        }
        Some(latest)
    }
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod journey400_316_saturation_tests {
    use super::*;

    #[test]
    fn every_counter_saturates_independently_without_affecting_queue_behavior() {
        let near = u64::MAX - 1;
        let mut queue = InputQueue {
            metrics: InputQueueMetrics {
                accepted_inputs: near,
                rejected_invalid: near,
                rejected_full: near,
                rejected_duplicate_newest: near,
                rejected_out_of_order: near,
                returned_inputs: near,
                coalesced_inputs: near,
                discarded_inputs: near,
            },
            ..InputQueue::default()
        };
        for sequence in 0..3 {
            assert!(queue.push(InputFrame {
                sequence,
                ..InputFrame::default()
            }));
        }
        for _ in 0..2 {
            assert!(!queue.push(InputFrame {
                move_x: f32::NAN,
                ..InputFrame::default()
            }));
            assert!(!queue.push(InputFrame {
                sequence: 2,
                ..InputFrame::default()
            }));
            assert!(!queue.push(InputFrame::default()));
        }
        assert_eq!(queue.pop_latest().unwrap().sequence, 2);
        for sequence in 3..3 + MAX_QUEUED_INPUTS as u32 {
            assert!(queue.push(InputFrame {
                sequence,
                ..InputFrame::default()
            }));
        }
        for _ in 0..2 {
            assert!(!queue.push(InputFrame {
                sequence: 100,
                ..InputFrame::default()
            }));
        }
        assert_eq!(queue.pop().unwrap().sequence, 3);
        queue.clear_pending();
        assert!(queue.push(InputFrame {
            sequence: 100,
            ..InputFrame::default()
        }));
        queue.reset_session();
        assert!(queue.is_empty());
        assert_eq!(
            queue.metrics(),
            InputQueueMetrics {
                accepted_inputs: u64::MAX,
                rejected_invalid: u64::MAX,
                rejected_full: u64::MAX,
                rejected_duplicate_newest: u64::MAX,
                rejected_out_of_order: u64::MAX,
                returned_inputs: u64::MAX,
                coalesced_inputs: u64::MAX,
                discarded_inputs: u64::MAX,
            }
        );
        assert_eq!(queue.reset_metrics().accepted_inputs, u64::MAX);
        assert_eq!(queue.metrics(), InputQueueMetrics::default());
        assert!(queue.push(InputFrame::default()));
        assert_eq!(queue.metrics().accepted_inputs, 1);
    }
}

/// Rejection from packet encoding. Field paths are stable diagnostic names;
/// indices identify a zero-based input/player/epoch, never a byte offset.
///
/// All field validation precedes output capacity checking. Legacy allocating
/// encoders map field/length errors to [`ProtocolError::Invalid`] and whole
/// packet overflow to [`ProtocolError::TooLarge`]. They never return Capacity.
/// See [`encode_client_into`] for a capacity/error example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    InvalidField {
        field: &'static str,
        index: Option<usize>,
    },
    Length {
        field: &'static str,
        len: usize,
        min: usize,
        max: usize,
    },
    TooLarge {
        required: usize,
        max: usize,
    },
    Capacity {
        required: usize,
        available: usize,
    },
}
impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ferravex packet encoding rejected: {self:?}")
    }
}
impl std::error::Error for EncodeError {}
impl EncodeError {
    fn protocol(self) -> ProtocolError {
        match self {
            Self::TooLarge { .. } | Self::Capacity { .. } => ProtocolError::TooLarge,
            Self::InvalidField { .. } | Self::Length { .. } => ProtocolError::Invalid,
        }
    }
}

// The same walker validates/counts with no destination, then writes to the
// preflighted slice. Immutable packet borrows prevent changes between passes.
// No unsafe aliases, allocation, stack packet copy or separate length formulas.
struct Encoder<'a> {
    destination: Option<&'a mut [u8]>,
    len: usize,
}
impl Encoder<'_> {
    fn extend_from_slice(&mut self, bytes: &[u8]) {
        if let Some(destination) = &mut self.destination {
            destination[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        }
        self.len += bytes.len();
    }
    fn push(&mut self, byte: u8) {
        self.extend_from_slice(&[byte]);
    }
    fn header(&mut self, kind: u8) {
        self.extend_from_slice(MAGIC);
        self.extend_from_slice(&[PROTOCOL_VERSION, kind]);
    }
}
fn invalid(field: &'static str, index: Option<usize>) -> EncodeError {
    EncodeError::InvalidField { field, index }
}
fn valid_field(ok: bool, field: &'static str, index: Option<usize>) -> Result<(), EncodeError> {
    if ok {
        Ok(())
    } else {
        Err(invalid(field, index))
    }
}
fn valid_len(field: &'static str, len: usize, min: usize, max: usize) -> Result<(), EncodeError> {
    if (min..=max).contains(&len) {
        Ok(())
    } else {
        Err(EncodeError::Length {
            field,
            len,
            min,
            max,
        })
    }
}
fn valid_session(session: u64) -> Result<(), EncodeError> {
    valid_field(session != 0, "session", None)
}
fn validate_input(input: &InputFrame, index: usize) -> Result<(), EncodeError> {
    for (value, max, field) in [
        (input.move_x, 1., "inputs.move_x"),
        (input.move_z, 1., "inputs.move_z"),
        (input.yaw, std::f32::consts::PI, "inputs.yaw"),
        (input.pitch, MAX_PITCH, "inputs.pitch"),
    ] {
        valid_field(value.is_finite() && value.abs() <= max, field, Some(index))?;
    }
    valid_field(
        input.buttons & !(BUTTON_JUMP | BUTTON_FIRE | BUTTON_RELOAD) == 0,
        "inputs.buttons",
        Some(index),
    )
}
fn validate_player(p: &PlayerSnapshot, index: usize) -> Result<(), EncodeError> {
    valid_field((p.id as usize) < MAX_PLAYERS, "players.id", Some(index))?;
    for (values, max, fields) in [
        (
            p.position,
            4096.,
            [
                "players.position.x",
                "players.position.y",
                "players.position.z",
            ],
        ),
        (
            p.velocity,
            256.,
            [
                "players.velocity.x",
                "players.velocity.y",
                "players.velocity.z",
            ],
        ),
    ] {
        for (value, field) in values.into_iter().zip(fields) {
            valid_field(value.is_finite() && value.abs() <= max, field, Some(index))?;
        }
    }
    for (value, max, field) in [
        (p.yaw, std::f32::consts::PI, "players.yaw"),
        (p.pitch, MAX_PITCH, "players.pitch"),
    ] {
        valid_field(value.is_finite() && value.abs() <= max, field, Some(index))?;
    }
    for (value, field) in [
        (p.jump_coyote, "players.jump_coyote"),
        (p.jump_buffer, "players.jump_buffer"),
    ] {
        valid_field(
            value.is_finite() && (0. ..=0.5).contains(&value),
            field,
            Some(index),
        )?;
    }
    for (value, max, field) in [
        (u16::from(p.health), 100, "players.health"),
        (u16::from(p.ammo), 30, "players.ammo"),
        (p.fire_cooldown, 4096, "players.fire_cooldown"),
        (p.reload_ticks, 4096, "players.reload_ticks"),
        (p.respawn_ticks, 4096, "players.respawn_ticks"),
    ] {
        valid_field(value <= max, field, Some(index))?;
    }
    Ok(())
}
fn planned_len(
    write: impl FnOnce(&mut Encoder<'_>) -> Result<(), EncodeError>,
) -> Result<usize, EncodeError> {
    let mut out = Encoder {
        destination: None,
        len: 0,
    };
    write(&mut out)?;
    if out.len > MAX_PACKET_BYTES {
        return Err(EncodeError::TooLarge {
            required: out.len,
            max: MAX_PACKET_BYTES,
        });
    }
    Ok(out.len)
}
fn encode_into(
    destination: &mut [u8],
    write: impl Fn(&mut Encoder<'_>) -> Result<(), EncodeError>,
) -> Result<usize, EncodeError> {
    let required = planned_len(&write)?;
    if required > destination.len() {
        return Err(EncodeError::Capacity {
            required,
            available: destination.len(),
        });
    }
    let mut out = Encoder {
        destination: Some(&mut destination[..required]),
        len: 0,
    };
    // All validation already succeeded on these same immutable fields. Keep the
    // second pass checked too; it cannot fail for a preflighted packet.
    write(&mut out)?;
    debug_assert_eq!(out.len, required);
    Ok(required)
}
fn encode_allocated(
    write: impl Fn(&mut Encoder<'_>) -> Result<(), EncodeError>,
) -> Result<Vec<u8>, ProtocolError> {
    let required = planned_len(&write).map_err(EncodeError::protocol)?;
    let mut bytes = vec![0; required];
    let mut out = Encoder {
        destination: Some(&mut bytes),
        len: 0,
    };
    write(&mut out).map_err(EncodeError::protocol)?;
    debug_assert_eq!(out.len, required);
    Ok(bytes)
}
fn put_u32(out: &mut Encoder<'_>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_u64(out: &mut Encoder<'_>, n: u64) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_f32(out: &mut Encoder<'_>, n: f32) {
    put_u32(out, n.to_bits());
}
fn put_u16(out: &mut Encoder<'_>, n: u16) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_input(out: &mut Encoder<'_>, input: &InputFrame) {
    put_u32(out, input.sequence);
    for n in [input.move_x, input.move_z, input.yaw, input.pitch] {
        put_f32(out, n);
    }
    out.push(input.buttons);
}

/// Exact v5 wire length after validating every client field; allocates nothing.
/// Includes the six-byte header and session/envelope overhead. See
/// [`encode_client_into`] for the shared validation/precedence contract.
///
/// ```
/// use ferravex_net::{ClientPacket, encoded_client_len};
/// assert_eq!(encoded_client_len(&ClientPacket::Hello { nonce: 7 }), Ok(14));
/// ```
pub fn encoded_client_len(packet: &ClientPacket) -> Result<usize, EncodeError> {
    planned_len(|out| write_client(packet, out))
}
/// Exact validated v5 server wire length, without allocation. Inner delta bytes
/// remain opaque: only outer tick ordering and byte length are checked, matching
/// the legacy encoder/decoder. Does not authenticate/install/ACK a snapshot.
///
/// ```
/// use ferravex_net::{ServerPacket, Snapshot, encoded_server_len};
/// let packet = ServerPacket::Snapshot(Snapshot { session: 1, ..Snapshot::default() });
/// assert_eq!(encoded_server_len(&packet), Ok(88));
/// ```
pub fn encoded_server_len(packet: &ServerPacket) -> Result<usize, EncodeError> {
    planned_len(|out| write_server(packet, out))
}
/// Encode v5 into a disjoint caller slice without allocating. Success initializes
/// exactly the returned prefix and leaves the tail untouched. Every error leaves
/// the **entire** destination unchanged. Capacity means slice length, not any
/// underlying Vec capacity; there is no internal scratch packet or retained state.
///
/// Error precedence: session first (Hello has none); collection length before
/// its elements; elements in slice order, fields in InputFrame::is_valid /
/// PlayerSnapshot::is_valid order; each player validity before duplicate ID;
/// snapshot orphan epochs in slot order after all players. Extension fields are
/// checked in wire order (delta tick order before byte length; pong client,
/// receive, send bounds before send ordering then five-second span). Whole packet
/// bound follows field validation; destination capacity is always last.
/// Unrestricted sequence/nonce/tick/shot_tick values remain unrestricted.
///
/// ```
/// use ferravex_net::{ClientPacket, EncodeError, encode_client_into};
/// let packet = ClientPacket::Hello { nonce: 7 };
/// let mut bytes = [99; 16];
/// assert_eq!(encode_client_into(&packet, &mut bytes[..13]),
///     Err(EncodeError::Capacity { required: 14, available: 13 }));
/// assert_eq!(bytes, [99; 16]);
/// let n = encode_client_into(&packet, &mut bytes).unwrap();
/// assert_eq!(n, 14);
/// assert_eq!(&bytes[n..], &[99; 2]);
/// ```
pub fn encode_client_into(
    packet: &ClientPacket,
    destination: &mut [u8],
) -> Result<usize, EncodeError> {
    encode_into(destination, |out| write_client(packet, out))
}
/// Server counterpart of [`encode_client_into`], with identical atomicity,
/// tail, allocation and capacity guarantees. Preserves exact v5 float bits.
///
/// ```
/// use ferravex_net::{ServerPacket, encode_server_into, decode_server, MAX_PACKET_BYTES};
/// let packet = ServerPacket::Welcome { nonce: 0, session: 1, player_id: 0, tick: 4 };
/// let mut bytes = [0; MAX_PACKET_BYTES];
/// let n = encode_server_into(&packet, &mut bytes).unwrap();
/// assert_eq!(decode_server(&bytes[..n]).unwrap(), packet);
/// ```
pub fn encode_server_into(
    packet: &ServerPacket,
    destination: &mut [u8],
) -> Result<usize, EncodeError> {
    encode_into(destination, |out| write_server(packet, out))
}
/// Allocating compatibility wrapper using the same validated v5 writer as
/// [`encode_client_into`]. Accepted inputs, bytes and ProtocolError mapping are
/// unchanged; returned Vec capacity is unspecified.
pub fn encode_client(packet: &ClientPacket) -> Result<Vec<u8>, ProtocolError> {
    encode_allocated(|out| write_client(packet, out))
}
/// Allocating compatibility wrapper for [`encode_server_into`].
pub fn encode_server(packet: &ServerPacket) -> Result<Vec<u8>, ProtocolError> {
    encode_allocated(|out| write_server(packet, out))
}

/// Borrowed v5 reliable/tactical envelope for an [`reliable::EventSender::send_due`]
/// callback. Uses the owned packet codec, without cloning an EventPacket or its
/// payload. Both wire families add **24 bytes** to the payload; reliable events
/// accept empty payloads, commands require 1..=900 bytes. No sequence policy or
/// application payload validation is added; validate the outer session and game
/// payload before receiver admission. Safe Rust requires disjoint output/input.
///
/// Supplied-time recipe: local refusal, accepted loss, out-of-order receipt,
/// individual wire ACKs, retry, and draining after downstream capacity reopens.
/// Decode/admission and initial sender queueing may allocate; encoding does not.
/// A real transport callback must return true only for a complete datagram or
/// durable outbound queue acceptance. WouldBlock and short sends return false;
/// retain fatal errors outside send_due and propagate them afterward. Local
/// acceptance never manufactures a remote ACK.
///
/// ```
/// use ferravex_net::{*, reliable::{EventSender, EventReceiver, EVENT_LIMIT}};
/// let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
/// sender.queue(b"first").unwrap();
/// sender.queue(b"second").unwrap();
/// let mut wire = [0; MAX_PACKET_BYTES];
/// let refused = sender.send_due(1000, 2, 11, |event| {
///     BorrowedEvent::ReliableEvent { session: 7, event }.encode_into(&mut wire).unwrap();
///     false // e.g. WouldBlock; no retry timestamp is committed.
/// });
/// assert_eq!(refused.sent, 0);
/// let mut ack_wire = [0; 22];
/// let mut ack_len = 0;
/// let report = sender.send_due(1001, 2, 11, |event| {
///     let envelope = BorrowedEvent::ReliableEvent { session: 7, event };
///     let n = envelope.encode_into(&mut wire).unwrap();
///     assert_eq!(n, envelope.encoded_len().unwrap());
///     if event.sequence == 0 { return true; } // Locally accepted, lost in transit.
///     let ServerPacket::ReliableEvent { session: 7, event } = decode_server(&wire[..n]).unwrap()
///         else { panic!("wrong session/family") };
///     receiver.admit(&event, EVENT_LIMIT).unwrap(); // Application validation first in FPS.
///     ack_len = encode_client_into(&ClientPacket::EventAck {
///         session: 7, sequence: event.sequence,
///     }, &mut ack_wire).unwrap();
///     true
/// });
/// assert_eq!(report.sent, 2);
/// let ClientPacket::EventAck { session: 7, sequence } = decode_client(&ack_wire[..ack_len]).unwrap()
///     else { panic!("wrong session/family") };
/// assert!(sender.acknowledge(sequence)); // Only the retained successor is ACKed.
/// assert_eq!(receiver.drain_ready(2, 11, |_, _| true).delivered, 0);
/// assert_eq!(sender.send_due(1100, 2, 11, |_| panic!("too early")).sent, 0);
/// sender.send_due(1101, 1, 5, |event| {
///     let n = BorrowedEvent::ReliableEvent { session: 7, event }.encode_into(&mut wire).unwrap();
///     let ServerPacket::ReliableEvent { session: 7, event } = decode_server(&wire[..n]).unwrap()
///         else { panic!("wrong session/family") };
///     receiver.admit(&event, EVENT_LIMIT).unwrap();
///     ack_len = encode_client_into(&ClientPacket::EventAck {
///         session: 7, sequence: event.sequence,
///     }, &mut ack_wire).unwrap();
///     true
/// });
/// let ClientPacket::EventAck { session: 7, sequence } = decode_client(&ack_wire[..ack_len]).unwrap()
///     else { panic!("wrong session/family") };
/// assert!(sender.acknowledge(sequence));
/// assert_eq!(sender.pending(), 0);
/// assert_eq!(receiver.drain_ready(2, 11, |_, _| false).delivered, 0);
/// // Later downstream capacity opens, without a new incoming packet.
/// assert_eq!(receiver.drain_ready(1, 5, |seq, bytes| { assert_eq!((seq, bytes), (0, b"first".as_slice())); true }).delivered, 1);
/// assert_eq!(receiver.drain_ready(1, 6, |seq, bytes| { assert_eq!((seq, bytes), (1, b"second".as_slice())); true }).delivered, 1);
/// ```
#[derive(Clone, Copy, Debug)]
pub enum BorrowedEvent<'a> {
    ReliableEvent {
        session: u64,
        event: &'a reliable::EventPacket,
    },
    Command {
        session: u64,
        event: &'a reliable::EventPacket,
    },
}
impl BorrowedEvent<'_> {
    /// Validated wire length, with the same precedence as [`encoded_client_len`].
    /// See the complete supplied-time example on [`BorrowedEvent`].
    pub fn encoded_len(self) -> Result<usize, EncodeError> {
        planned_len(|out| self.write(out))
    }
    /// Checked caller-buffer encoding; shares [`encode_client_into`]'s full
    /// failure atomicity and untouched-tail guarantees. No clone/allocation.
    ///
    /// ```
    /// use ferravex_net::{BorrowedEvent, ClientExtension, ClientPacket, decode_client};
    /// use ferravex_net::reliable::EventPacket;
    /// let event = EventPacket { sequence: 3, payload: vec![1, 2] };
    /// let envelope = BorrowedEvent::Command { session: 7, event: &event };
    /// let mut wire = [99; 28];
    /// let n = envelope.encode_into(&mut wire).unwrap();
    /// assert_eq!(n, envelope.encoded_len().unwrap());
    /// assert_eq!(&wire[n..], &[99; 2]);
    /// assert_eq!(decode_client(&wire[..n]).unwrap(),
    ///     ClientPacket::Extension(ClientExtension::Command { session: 7, event }));
    /// ```
    pub fn encode_into(self, destination: &mut [u8]) -> Result<usize, EncodeError> {
        encode_into(destination, |out| self.write(out))
    }
    fn write(self, out: &mut Encoder<'_>) -> Result<(), EncodeError> {
        let (kind, session, event, min) = match self {
            Self::ReliableEvent { session, event } => (5, session, event, 0),
            Self::Command { session, event } => (10, session, event, 1),
        };
        valid_session(session)?;
        valid_len(
            "event.payload",
            event.payload.len(),
            min,
            reliable::EVENT_BYTES,
        )?;
        out.header(kind);
        put_u64(out, session);
        put_u64(out, event.sequence);
        put_u16(out, event.payload.len() as u16);
        out.extend_from_slice(&event.payload);
        Ok(())
    }
}

fn write_client(packet: &ClientPacket, out: &mut Encoder<'_>) -> Result<(), EncodeError> {
    match packet {
        ClientPacket::Extension(p) => return wire_v4::write_client(p, out),
        ClientPacket::EventAck { session, sequence } => {
            valid_session(*session)?;
            out.header(6);
            put_u64(out, *session);
            put_u64(out, *sequence);
        }
        ClientPacket::Hello { nonce } => {
            out.header(1);
            put_u64(out, *nonce);
        }
        ClientPacket::Input { session, inputs } => {
            valid_session(*session)?;
            valid_len("inputs", inputs.len(), 1, MAX_INPUTS_PER_PACKET)?;
            out.header(2);
            put_u64(out, *session);
            out.push(inputs.len() as u8);
            for (index, input) in inputs.iter().enumerate() {
                validate_input(input, index)?;
                put_input(out, input);
            }
        }
    }
    Ok(())
}
fn write_server(packet: &ServerPacket, out: &mut Encoder<'_>) -> Result<(), EncodeError> {
    match packet {
        ServerPacket::Extension(p) => return wire_v4::write_server(p, out),
        ServerPacket::ReliableEvent { session, event } => {
            return BorrowedEvent::ReliableEvent {
                session: *session,
                event,
            }
            .write(out);
        }
        ServerPacket::Welcome {
            nonce,
            session,
            player_id,
            tick,
        } => {
            valid_session(*session)?;
            valid_field((*player_id as usize) < MAX_PLAYERS, "player_id", None)?;
            out.header(3);
            put_u64(out, *nonce);
            put_u64(out, *session);
            out.push(*player_id);
            put_u32(out, *tick);
        }
        ServerPacket::Snapshot(snapshot) => {
            valid_session(snapshot.session)?;
            valid_len("players", snapshot.players.len(), 0, MAX_PLAYERS)?;
            let mut ids = 0u16;
            for (index, p) in snapshot.players.iter().enumerate() {
                validate_player(p, index)?;
                valid_field(
                    ids & (1u16 << p.id) == 0,
                    "players.duplicate_id",
                    Some(index),
                )?;
                ids |= 1u16 << p.id;
            }
            for (i, epoch) in snapshot.epochs.iter().enumerate() {
                valid_field(*epoch == 0 || ids & (1 << i) != 0, "epochs.orphan", Some(i))?;
            }
            out.header(4);
            put_u64(out, snapshot.session);
            put_u32(out, snapshot.tick);
            put_u32(out, snapshot.ack_sequence);
            out.push(snapshot.ack_valid as u8);
            for epoch in snapshot.epochs {
                put_u32(out, epoch);
            }
            out.push(snapshot.players.len() as u8);
            for p in &snapshot.players {
                out.push(p.id);
                for n in p
                    .position
                    .into_iter()
                    .chain(p.velocity)
                    .chain([p.yaw, p.pitch])
                {
                    put_f32(out, n);
                }
                out.extend_from_slice(&[p.health, p.ammo, p.grounded as u8]);
                put_f32(out, p.jump_coyote);
                put_f32(out, p.jump_buffer);
                out.push(p.jump_held as u8);
                put_u16(out, p.fire_cooldown);
                put_u16(out, p.reload_ticks);
                put_u16(out, p.respawn_ticks);
            }
        }
    }
    Ok(())
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Result<(Self, u8), ProtocolError> {
        if data.len() > MAX_PACKET_BYTES {
            return Err(ProtocolError::TooLarge);
        }
        if data.len() < 6 {
            return Err(ProtocolError::Truncated);
        }
        if &data[..4] != MAGIC {
            return Err(ProtocolError::Header);
        }
        if data[4] != PROTOCOL_VERSION {
            return Err(ProtocolError::Version);
        }
        Ok((Self { data, pos: 6 }, data[5]))
    }
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], ProtocolError> {
        let bytes = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or(ProtocolError::Truncated)?;
        self.pos += N;
        Ok(bytes.try_into().expect("slice length checked"))
    }
    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.bytes::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(self.bytes()?))
    }
    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn f32(&mut self) -> Result<f32, ProtocolError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn boolean(&mut self) -> Result<bool, ProtocolError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::Invalid),
        }
    }
    fn end(self) -> Result<(), ProtocolError> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(ProtocolError::Trailing)
        }
    }
    fn input(&mut self) -> Result<InputFrame, ProtocolError> {
        let frame = InputFrame {
            sequence: self.u32()?,
            move_x: self.f32()?,
            move_z: self.f32()?,
            yaw: self.f32()?,
            pitch: self.f32()?,
            buttons: self.u8()?,
        };
        if !frame.is_valid() {
            return Err(ProtocolError::Invalid);
        }
        Ok(frame)
    }
}
pub fn decode_client(data: &[u8]) -> Result<ClientPacket, ProtocolError> {
    let (mut reader, kind) = Reader::new(data)?;
    let packet = match kind {
        7..=10 => ClientPacket::Extension(wire_v4::decode_client(&mut reader, kind)?),
        6 => {
            let session = reader.u64()?;
            let sequence = reader.u64()?;
            if session == 0 {
                return Err(ProtocolError::Invalid);
            }
            ClientPacket::EventAck { session, sequence }
        }
        1 => ClientPacket::Hello {
            nonce: reader.u64()?,
        },
        2 => {
            let session = reader.u64()?;
            let count = reader.u8()? as usize;
            if session == 0 || count == 0 || count > MAX_INPUTS_PER_PACKET {
                return Err(ProtocolError::Invalid);
            }
            let mut inputs = Vec::with_capacity(count);
            for _ in 0..count {
                inputs.push(reader.input()?);
            }
            ClientPacket::Input { session, inputs }
        }
        _ => return Err(ProtocolError::Kind),
    };
    reader.end()?;
    Ok(packet)
}
pub fn decode_server(data: &[u8]) -> Result<ServerPacket, ProtocolError> {
    let (mut r, kind) = Reader::new(data)?;
    let packet = match kind {
        11..=13 => ServerPacket::Extension(wire_v4::decode_server(&mut r, kind)?),
        5 => {
            let session = r.u64()?;
            let sequence = r.u64()?;
            let len = r.u16()? as usize;
            if session == 0 || len > reliable::EVENT_BYTES {
                return Err(ProtocolError::Invalid);
            }
            let payload = r
                .data
                .get(r.pos..r.pos + len)
                .ok_or(ProtocolError::Truncated)?
                .to_vec();
            r.pos += len;
            ServerPacket::ReliableEvent {
                session,
                event: reliable::EventPacket { sequence, payload },
            }
        }
        3 => {
            let nonce = r.u64()?;
            let session = r.u64()?;
            let player_id = r.u8()?;
            let tick = r.u32()?;
            if session == 0 || player_id as usize >= MAX_PLAYERS {
                return Err(ProtocolError::Invalid);
            }
            ServerPacket::Welcome {
                nonce,
                session,
                player_id,
                tick,
            }
        }
        4 => {
            let session = r.u64()?;
            if session == 0 {
                return Err(ProtocolError::Invalid);
            }
            let tick = r.u32()?;
            let ack_sequence = r.u32()?;
            let ack_valid = r.boolean()?;
            let mut epochs = [0; MAX_PLAYERS];
            for epoch in &mut epochs {
                *epoch = r.u32()?;
            }
            let count = r.u8()? as usize;
            if count > MAX_PLAYERS {
                return Err(ProtocolError::Invalid);
            }
            let mut players = Vec::with_capacity(count);
            let mut ids = 0u16;
            for _ in 0..count {
                let p = PlayerSnapshot {
                    id: r.u8()?,
                    position: [r.f32()?, r.f32()?, r.f32()?],
                    velocity: [r.f32()?, r.f32()?, r.f32()?],
                    yaw: r.f32()?,
                    pitch: r.f32()?,
                    health: r.u8()?,
                    ammo: r.u8()?,
                    grounded: r.boolean()?,
                    jump_coyote: r.f32()?,
                    jump_buffer: r.f32()?,
                    jump_held: r.boolean()?,
                    fire_cooldown: r.u16()?,
                    reload_ticks: r.u16()?,
                    respawn_ticks: r.u16()?,
                };
                if !p.is_valid() || ids & (1u16 << p.id) != 0 {
                    return Err(ProtocolError::Invalid);
                }
                ids |= 1u16 << p.id;
                players.push(p);
            }
            if epochs
                .iter()
                .enumerate()
                .any(|(i, e)| *e != 0 && ids & (1 << i) == 0)
            {
                return Err(ProtocolError::Invalid);
            }
            ServerPacket::Snapshot(Snapshot {
                session,
                tick,
                ack_sequence,
                ack_valid,
                epochs,
                players,
            })
        }
        _ => return Err(ProtocolError::Kind),
    };
    r.end()?;
    Ok(packet)
}

/// Native sockets are optional by target; packet codecs remain available on wasm.
#[cfg(not(target_arch = "wasm32"))]
pub mod udp {
    use super::MAX_PACKET_BYTES;
    use std::{
        io,
        net::{SocketAddr, ToSocketAddrs, UdpSocket},
    };
    pub struct Socket {
        inner: UdpSocket,
    }
    impl Socket {
        pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
            let inner = UdpSocket::bind(addr)?;
            inner.set_nonblocking(true)?;
            Ok(Self { inner })
        }
        pub fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
            let remote = addr.to_socket_addrs()?.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "empty remote address")
            })?;
            let this = Self::bind(if remote.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })?;
            this.inner.connect(remote)?;
            Ok(this)
        }
        pub fn local_addr(&self) -> io::Result<SocketAddr> {
            self.inner.local_addr()
        }
        pub fn send(&self, bytes: &[u8]) -> io::Result<usize> {
            check_size(bytes)?;
            self.inner.send(bytes)
        }
        pub fn send_to(&self, bytes: &[u8], addr: SocketAddr) -> io::Result<usize> {
            check_size(bytes)?;
            self.inner.send_to(bytes, addr)
        }
        /// The extra byte detects oversized/truncated UDP datagrams. Pass the
        /// result to the codec, which rejects lengths above MAX_PACKET_BYTES.
        /// Windows discards oversized datagrams and returns InvalidData; callers
        /// should count those as rejected packets and continue their poll budget.
        /// Only an empty socket returns None.
        pub fn recv_from(
            &self,
            buffer: &mut [u8; MAX_PACKET_BYTES + 1],
        ) -> io::Result<Option<(usize, SocketAddr)>> {
            match self.inner.recv_from(buffer) {
                Ok(pair) => Ok(Some(pair)),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                #[cfg(windows)]
                Err(e) if e.raw_os_error() == Some(10040) => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "oversized UDP datagram discarded by Winsock",
                )),
                Err(e) => Err(e),
            }
        }
    }
    fn check_size(bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_PACKET_BYTES {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "packet exceeds MTU budget",
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codec_round_trip_and_budget() {
        let frame = InputFrame {
            sequence: u32::MAX,
            move_x: 0.5,
            move_z: -1.0,
            yaw: 2.0,
            pitch: 0.1,
            buttons: 7,
        };
        for packet in [
            ClientPacket::Hello { nonce: 123 },
            ClientPacket::Input {
                session: 99,
                inputs: vec![frame; MAX_INPUTS_PER_PACKET],
            },
        ] {
            assert_eq!(
                decode_client(&encode_client(&packet).unwrap()).unwrap(),
                packet
            );
        }
        let players = (0..MAX_PLAYERS)
            .map(|id| PlayerSnapshot {
                id: id as u8,
                health: 100,
                ammo: 30,
                jump_coyote: 0.1,
                jump_buffer: 0.12,
                jump_held: true,
                ..Default::default()
            })
            .collect();
        let packet = ServerPacket::Snapshot(Snapshot {
            epochs: [0; MAX_PLAYERS],
            session: 99,
            tick: 44,
            ack_valid: true,
            ack_sequence: 3,
            players,
        });
        let bytes = encode_server(&packet).unwrap();
        assert_eq!(bytes.len(), 904);
        assert!(bytes.len() < MAX_PACKET_BYTES);
        assert_eq!(decode_server(&bytes).unwrap(), packet);
        for length in 0..bytes.len() {
            assert!(decode_server(&bytes[..length]).is_err());
        }
    }
    #[test]
    fn jump_state_numeric_and_version_limits_fail_closed() {
        let player = PlayerSnapshot {
            health: 100,
            ammo: 30,
            ..Default::default()
        };
        let make = |p| {
            ServerPacket::Snapshot(Snapshot {
                epochs: [0; MAX_PLAYERS],
                session: 1,
                players: vec![p],
                ..Default::default()
            })
        };
        for bad in [f32::NAN, f32::INFINITY, -0.01, 0.51] {
            assert!(
                encode_server(&make(PlayerSnapshot {
                    jump_coyote: bad,
                    ..player
                }))
                .is_err()
            );
            assert!(
                encode_server(&make(PlayerSnapshot {
                    jump_buffer: bad,
                    ..player
                }))
                .is_err()
            );
        }
        let mut bytes = encode_server(&make(player)).unwrap();
        bytes[4] = 2;
        assert_eq!(decode_server(&bytes), Err(ProtocolError::Version));
    }
    #[test]
    fn reliable_event_wire_is_bounded_session_bound_and_versioned() {
        let ack = ClientPacket::EventAck {
            session: 77,
            sequence: 123,
        };
        assert_eq!(decode_client(&encode_client(&ack).unwrap()).unwrap(), ack);
        let packet = ServerPacket::ReliableEvent {
            session: 77,
            event: reliable::EventPacket {
                sequence: 123,
                payload: vec![4; reliable::EVENT_BYTES],
            },
        };
        let bytes = encode_server(&packet).unwrap();
        assert!(bytes.len() <= MAX_PACKET_BYTES);
        assert_eq!(decode_server(&bytes).unwrap(), packet);
        for n in 0..bytes.len() {
            assert!(decode_server(&bytes[..n]).is_err());
        }
        let mut old = bytes.clone();
        old[4] = 1;
        assert_eq!(decode_server(&old), Err(ProtocolError::Version));
        assert!(
            encode_client(&ClientPacket::EventAck {
                session: 0,
                sequence: 1
            })
            .is_err()
        );
        assert!(
            encode_server(&ServerPacket::ReliableEvent {
                session: 77,
                event: reliable::EventPacket {
                    sequence: 0,
                    payload: vec![0; reliable::EVENT_BYTES + 1]
                }
            })
            .is_err()
        );
    }
    #[test]
    fn reject_bad_numeric_and_trailing_data() {
        let p = ClientPacket::Input {
            session: 3,
            inputs: vec![InputFrame {
                move_x: f32::NAN,
                ..Default::default()
            }],
        };
        assert!(encode_client(&p).is_err());
        let mut bytes = encode_client(&ClientPacket::Hello { nonce: 5 }).unwrap();
        bytes.push(0);
        assert_eq!(decode_client(&bytes), Err(ProtocolError::Trailing));
        let frame = InputFrame {
            pitch: f32::INFINITY,
            ..Default::default()
        };
        assert!(frame.sanitized().is_valid());
    }
    #[test]
    fn queue_is_bounded_deduplicated_and_wrap_safe() {
        assert!(sequence_newer(0, u32::MAX));
        assert!(!sequence_newer(u32::MAX, 0));
        assert!(!sequence_newer(1 << 31, 0));
        let mut q = InputQueue::default();
        assert!(q.push(InputFrame {
            sequence: u32::MAX,
            ..Default::default()
        }));
        assert!(q.push(InputFrame::default()));
        assert!(!q.push(InputFrame::default()));
        for seq in 1..MAX_QUEUED_INPUTS as u32 {
            q.push(InputFrame {
                sequence: seq,
                ..Default::default()
            });
        }
        assert_eq!(q.len(), MAX_QUEUED_INPUTS);
        assert!(!q.push(InputFrame {
            sequence: 200,
            ..Default::default()
        }));
    }
    #[test]
    fn coalescing_keeps_latest_movement_and_recovers_action_press() {
        let mut queue = InputQueue::default();
        queue.push(InputFrame {
            sequence: 1,
            move_z: 1.,
            buttons: BUTTON_FIRE,
            ..Default::default()
        });
        queue.push(InputFrame {
            sequence: 2,
            move_x: 1.,
            ..Default::default()
        });
        let input = queue.pop_latest().unwrap();
        assert_eq!(input.sequence, 2);
        assert_eq!(input.move_z, 0.);
        assert_eq!(input.move_x, 1.);
        assert_eq!(input.buttons, BUTTON_FIRE);
        assert!(queue.is_empty());
        assert!(!queue.push(InputFrame {
            sequence: 1,
            buttons: BUTTON_FIRE,
            ..Default::default()
        }));
    }
    #[test]
    fn arbitrary_bytes_never_panic() {
        let mut state = 0x12345678u32;
        for len in 0..=MAX_PACKET_BYTES + 5 {
            let bytes: Vec<_> = (0..len)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect();
            let _ = decode_client(&bytes);
            let _ = decode_server(&bytes);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_udp_actual_socket_round_trip() {
        use std::{thread, time::Duration};
        let server = udp::Socket::bind("127.0.0.1:0").unwrap();
        let client = udp::Socket::connect(server.local_addr().unwrap()).unwrap();
        let bytes = encode_client(&ClientPacket::Hello { nonce: 88 }).unwrap();
        client.send(&bytes).unwrap();
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for _ in 0..100 {
            if let Some((n, peer)) = server.recv_from(&mut buffer).unwrap() {
                assert_eq!(
                    decode_client(&buffer[..n]).unwrap(),
                    ClientPacket::Hello { nonce: 88 }
                );
                server.send_to(&buffer[..n], peer).unwrap();
                for _ in 0..100 {
                    if let Some((n, _)) = client.recv_from(&mut buffer).unwrap() {
                        assert_eq!(
                            decode_client(&buffer[..n]).unwrap(),
                            ClientPacket::Hello { nonce: 88 }
                        );
                        return;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                panic!("loopback UDP response not received");
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("loopback UDP packet not received");
    }
}

#[cfg(test)]
mod timed_queue_tests {
    use super::*;
    #[test]
    fn round_reset_rejects_delayed_controls_from_previous_round() {
        let mut q = InputQueue::default();
        assert!(q.push(InputFrame {
            sequence: 42,
            buttons: BUTTON_FIRE,
            ..Default::default()
        }));
        q.clear_pending();
        assert!(q.pop().is_none());
        assert!(!q.push(InputFrame {
            sequence: 42,
            buttons: BUTTON_FIRE,
            ..Default::default()
        }));
        assert!(!q.push(InputFrame {
            sequence: 41,
            ..Default::default()
        }));
        assert!(q.push(InputFrame {
            sequence: 43,
            ..Default::default()
        }));
        assert_eq!(q.pop_latest().unwrap().buttons, 0);
    }
    #[test]
    fn recovered_fire_keeps_old_aim_and_time_but_latest_movement() {
        let mut q = InputQueue::default();
        q.push_timed(TimedInput {
            input: InputFrame {
                sequence: 1,
                yaw: 1.,
                buttons: BUTTON_FIRE,
                ..Default::default()
            },
            shot_tick: Some(20),
        });
        q.push_timed(TimedInput {
            input: InputFrame {
                sequence: 2,
                yaw: 2.,
                move_x: 1.,
                ..Default::default()
            },
            shot_tick: None,
        });
        let v = q.pop_latest_timed().unwrap();
        assert_eq!(v.input.move_x, 1.);
        assert_eq!(v.input.yaw, 1.);
        assert_eq!(v.shot_tick, Some(20));
        assert_eq!(v.input.sequence, 2);
        assert_eq!(v.input.buttons, BUTTON_FIRE);
        assert!(q.is_empty());
    }
}
