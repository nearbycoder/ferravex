//! Transport-independent v5 authority, used by both native and Wasm GameServer.
//!
//! Time is a caller-supplied, monotonic microsecond Duration in one domain.
//! Equal times are allowed; backwards, sub-microsecond and >2^53-us times reject
//! before protocol/game mutation. Poll performs maintenance and outgoing work;
//! tick executes exactly one fixed simulation step, independently of output
//! admission. Neither receives packets implicitly. Receive processes one bounded
//! datagram. Call poll regularly even without incoming traffic.
//!
//! Output callbacks accept an entire initialized encoded prefix or refuse it.
//! Refusal stops subsequent offers in that operation; accepted external effects
//! are never rolled back. Reliable packets remain due on refusal and untouched
//! tails; snapshots become ACK-authorizable only after accepted output and exact
//! full-byte retention. Welcome retries use another Hello after 250 ms.
//!
//! Bounds are explicit: <=16 peers; one <=1200-byte receive per call; <=6
//! inputs per datagram and <=128 queued inputs per peer; <=8 ordered commands
//! and <=8 reliable offers per peer per poll/tick. OutputBudget further caps
//! callbacks/accepted bytes to <=128 datagrams / 153600 bytes per operation.
//! Each peer retains <=32 full snapshots / 38400 bytes, <=256 reliable outgoing
//! payloads (<=900 bytes each), <=256 durable incoming commands (validated
//! two-byte actions), and one completed action-result slot. The whole receive
//! window avoids a successors-before-head admission deadlock. Existing combined
//! backlog overflow still disconnects slow peers when queuing tick events.
//!
//! These are logical storage/work bounds, not an allocation-free claim: packet
//! decoding, snapshot player collection, Match's legacy stepping, reliable
//! admission/result encoding and initial peer setup can allocate. Public raw
//! Match mutations must preserve its existing roster/state constraints.
//! Reconstruct ServerState for a full authority reset; rehello resets only the
//! peer session/input/reliable/baseline state and preserves the actual player.
//! The Hello interval gates accepted incoming handshakes/churn even when an
//! outgoing Welcome was refused; it is separate from reliable send retry times.
//!
//! ```
//! use ferravex_fps::{Arena, GameServer};
//! // This is also the pure state's type behind the public adapter.
//! type ServerState = <GameServer as std::ops::Deref>::Target;
//! use ferravex_net::{ClientPacket, ServerPacket, encode_client, decode_server};
//! use std::{net::SocketAddr, time::Duration};
//! let peer: SocketAddr = "127.0.0.1:9000".parse().unwrap();
//! let mut authority = ServerState::new(Arena::default());
//! let hello = encode_client(&ClientPacket::Hello { nonce: 42 }).unwrap();
//! let mut datagrams = Vec::new(); // Caller-owned bounded admission policy.
//! let report = authority.receive(peer, &hello, Duration::ZERO, || 7,
//!     Default::default(), |to, bytes| {
//!         if datagrams.len() == 2 { return false; }
//!         datagrams.push((to, bytes.to_vec())); true
//!     }).unwrap();
//! assert_eq!(report.datagrams_accepted, 2);
//! assert!(matches!(decode_server(&datagrams[0].1).unwrap(),
//!     ServerPacket::Welcome { session: 7, .. }));
//! let tick = authority.tick(&[], Duration::from_micros(15_625),
//!     Default::default(), |_, _| false).unwrap();
//! assert!(tick.report.tick_executed);
//! assert_eq!(authority.game.tick, 1);
//! let report = authority.poll(Duration::from_micros(15_625),
//!     Default::default(), |_, _| true).unwrap();
//! assert!(!report.tick_executed);
//! ```
use super::{
    EVENT_SEND_BUDGET, HELLO_INTERVAL, PEER_TIMEOUT, POLL_BUDGET, SNAPSHOT_INTERVAL,
    TacticalOptions,
};
use crate::tactical::Mode;
use crate::{Arena, InputFrame, MAX_PLAYERS, Match, NetworkEvent};
use ferravex_net::baselines::{BaselineError, BaselineLimits, SenderBaselines};
use ferravex_net::delta::Delta;
use ferravex_net::reliable::{EventReceiver, EventSender};
use ferravex_net::{
    BorrowedEvent, ClientPacket, EncodeError, InputAdmissionError, InputQueue, MAX_PACKET_BYTES,
    ServerPacket, decode_client, encode_server_into,
};
use std::{net::SocketAddr, time::Duration};

/// Whole-datagram callback/byte limits per operation, clamped to 128 packets and
/// 128 * MAX_PACKET_BYTES. Zero is valid. Budget exhaustion defers output without
/// invoking the callback or marking reliable retry times. The first nonfitting
/// packet stops further offers; no smaller tail is packed ahead of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputBudget {
    pub datagrams: usize,
    pub bytes: usize,
}
impl Default for OutputBudget {
    fn default() -> Self {
        Self {
            datagrams: POLL_BUDGET,
            bytes: POLL_BUDGET * MAX_PACKET_BYTES,
        }
    }
}
/// Exact work for a completed operation, or its committed prefix on failure via
/// ServerState::last_report. Refused means a callback returned false; deferred
/// means a generated datagram was not offered. Reliable remaining_due is the
/// complete due backlog sampled by poll/tick, not just generated datagrams.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkReport {
    pub datagrams_accepted: usize,
    pub datagrams_refused: usize,
    pub datagrams_deferred: usize,
    pub bytes_accepted: usize,
    pub packets_received: usize,
    pub packets_rejected: usize,
    pub inputs_accepted: usize,
    /// Remote queued samples selected by an actual tick (at most one per peer).
    pub inputs_consumed: usize,
    /// Older queued samples merged into those selected samples.
    pub inputs_coalesced: usize,
    /// Remote (including held) plus explicit local actor commands in this tick.
    pub authority_inputs: usize,
    pub inputs_refused: usize,
    pub inputs_invalid: usize,
    pub inputs_full: usize,
    pub inputs_duplicate: usize,
    pub inputs_out_of_order: usize,
    pub commands_retained: usize,
    pub commands_duplicate: usize,
    pub commands_refused: usize,
    pub commands_executed: usize,
    pub reliable_remaining_due: usize,
    pub peers_timed_out: usize,
    pub tick_executed: bool,
    pub errors: usize,
}
/// A completed simulation step. Events are accepted Match events in original
/// order; output refusal does not undo this tick. Allocates like the legacy API.
pub struct TickOutput {
    pub events: Vec<crate::GameEvent>,
    pub report: WorkReport,
}
/// Explicit authority errors. Invalid packets are counted in WorkReport rather
/// than aborting an operation. Inspect last_report after a returned error.
#[derive(Debug)]
#[non_exhaustive]
pub enum DriverError {
    InvalidTime,
    BackwardsTime,
    InvalidCookie,
    InvalidLocalInputs,
    TacticalConfiguration,
    Protocol(ferravex_net::ProtocolError),
    Encode(EncodeError),
    Baseline(BaselineError),
    Reliable(ferravex_net::reliable::Error),
}
impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authority driver: {self:?}")
    }
}
impl std::error::Error for DriverError {}
impl From<EncodeError> for DriverError {
    fn from(e: EncodeError) -> Self {
        Self::Encode(e)
    }
}
impl From<BaselineError> for DriverError {
    fn from(e: BaselineError) -> Self {
        Self::Baseline(e)
    }
}

pub(super) struct Output<'a> {
    pub(super) report: WorkReport,
    budget: OutputBudget,
    stopped: bool,
    send: &'a mut dyn FnMut(SocketAddr, &[u8]) -> bool,
}
impl<'a> Output<'a> {
    fn new(budget: OutputBudget, send: &'a mut dyn FnMut(SocketAddr, &[u8]) -> bool) -> Self {
        Self {
            report: WorkReport::default(),
            budget: OutputBudget {
                datagrams: budget.datagrams.min(POLL_BUDGET),
                bytes: budget.bytes.min(POLL_BUDGET * MAX_PACKET_BYTES),
            },
            stopped: false,
            send,
        }
    }
    pub(super) fn offer(&mut self, addr: SocketAddr, bytes: &[u8]) -> bool {
        if self.stopped
            || self.report.datagrams_accepted == self.budget.datagrams
            || bytes.len() > self.budget.bytes - self.report.bytes_accepted
        {
            self.stopped = true;
            self.report.datagrams_deferred += 1;
            return false;
        }
        if !(self.send)(addr, bytes) {
            self.report.datagrams_refused += 1;
            self.stopped = true;
            return false;
        }
        self.report.datagrams_accepted += 1;
        self.report.bytes_accepted += bytes.len();
        true
    }
    pub(super) fn packet(
        &mut self,
        addr: SocketAddr,
        packet: &ServerPacket,
    ) -> Result<bool, DriverError> {
        let mut bytes = [0; MAX_PACKET_BYTES];
        let n = encode_server_into(packet, &mut bytes)?;
        Ok(self.offer(addr, &bytes[..n]))
    }
}

pub(super) struct Peer {
    pub(super) addr: SocketAddr,
    pub(super) nonce: u64,
    pub(super) session: u64,
    pub(super) id: u8,
    pub(super) inputs: InputQueue,
    pub(super) last_input: InputFrame,
    pub(super) last_shot: Option<u32>,
    pub(super) snapshots: SenderBaselines,
    pub(super) delta: Delta,
    pub(super) last_ping: Option<Duration>,
    pub(super) commands: EventReceiver,
    pub(super) pending_result: Option<crate::tactical::SessionEvent>,
    pub(super) last_input_tick: u32,
    pub(super) ack: Option<u32>,
    pub(super) last_seen: Duration,
    pub(super) last_hello: Duration,
    pub(super) events: EventSender,
}

/// Pure bounded authority state. Public game/counter access preserves the host
/// interface; callers remain responsible for valid Match mutations. No socket,
/// clock reads, entropy, sleeps or platform I/O are used by this state.
pub struct ServerState {
    pub(super) peers: Vec<Peer>,
    pub game: Match,
    pub rejected_packets: u64,
    pub slow_event_disconnects: u64,
    pub rejected_shots: u64,
    pub(super) tactical_options: Option<TacticalOptions>,
    pub(super) tactical: Option<Mode>,
    pub(super) round_dirty: bool,
    pub(super) now: Duration,
    pub(super) last_report: WorkReport,
}
impl ServerState {
    /// Construct practice authority with the existing authored defaults.
    /// See the module's executable codec/handshake/tick recipe.
    pub fn new(arena: Arena) -> Self {
        Self {
            peers: Vec::with_capacity(MAX_PLAYERS),
            game: Match::new(arena),
            rejected_packets: 0,
            slow_event_disconnects: 0,
            rejected_shots: 0,
            tactical_options: None,
            tactical: None,
            round_dirty: false,
            now: Duration::ZERO,
            last_report: WorkReport::default(),
        }
    }
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    pub fn pending_event_count(&self) -> usize {
        self.peers
            .iter()
            .map(|p| p.events.pending() + usize::from(p.pending_result.is_some()))
            .sum()
    }
    pub fn pending_command_count(&self) -> usize {
        self.peers.iter().map(|p| p.commands.buffered()).sum()
    }
    /// Last completed/partially failed operation, including tick commitment and
    /// prior accepted callbacks. Configuration methods do not replace it.
    pub fn last_report(&self) -> WorkReport {
        self.last_report
    }
    fn check_time(&mut self, now: Duration) -> Result<(), DriverError> {
        if now > Duration::from_micros(1 << 53) || !now.subsec_nanos().is_multiple_of(1000) {
            return Err(DriverError::InvalidTime);
        }
        if now < self.now {
            return Err(DriverError::BackwardsTime);
        }
        self.now = now;
        Ok(())
    }
    fn finish<T>(
        &mut self,
        mut report: WorkReport,
        result: Result<T, DriverError>,
    ) -> Result<T, DriverError> {
        if result.is_err() {
            report.errors += 1;
        }
        self.rejected_packets = self
            .rejected_packets
            .saturating_add(report.packets_rejected as u64);
        self.last_report = report;
        result
    }
    /// Decode one datagram (<=1200 bytes) and dispatch authenticated authority
    /// work. Cookie supply is invoked only for a newly admitted/restarted session;
    /// it must return a nonzero cookie. No implicit tick or network receive.
    pub fn receive(
        &mut self,
        peer: SocketAddr,
        bytes: &[u8],
        now: Duration,
        mut cookie: impl FnMut() -> u64,
        budget: OutputBudget,
        mut send: impl FnMut(SocketAddr, &[u8]) -> bool,
    ) -> Result<WorkReport, DriverError> {
        let mut output = Output::new(budget, &mut send);
        let result = self.check_time(now).and_then(|()| {
            output.report.packets_received = 1;
            match decode_client(bytes) {
                Ok(packet) => self.receive_packet(peer, packet, now, &mut cookie, &mut output),
                Err(_) => {
                    output.report.packets_rejected += 1;
                    Ok(())
                }
            }
        });
        self.finish(output.report, result)?;
        Ok(self.last_report)
    }
    /// Maintenance without a simulation step: timeout peers, drain durable
    /// commands into available result space, and send due reliable datagrams.
    /// At most eight commands and eight reliable sends per peer per call.
    pub fn poll(
        &mut self,
        now: Duration,
        budget: OutputBudget,
        mut send: impl FnMut(SocketAddr, &[u8]) -> bool,
    ) -> Result<WorkReport, DriverError> {
        let mut output = Output::new(budget, &mut send);
        let result = self.check_time(now).and_then(|()| {
            self.expire(now, &mut output);
            self.drain_commands(&mut output)?;
            self.flush_events(now, &mut output)
        });
        self.finish(output.report, result)?;
        Ok(self.last_report)
    }
    /// Execute exactly one fixed tick with local inputs. Validation precedes
    /// maintenance/simulation. A refused output is successful tick completion;
    /// later encode errors return Err with last_report.tick_executed=true.
    /// There is no implicit receive. Input commands advance only on this method.
    pub fn tick(
        &mut self,
        local: &[(u8, InputFrame)],
        now: Duration,
        budget: OutputBudget,
        mut send: impl FnMut(SocketAddr, &[u8]) -> bool,
    ) -> Result<TickOutput, DriverError> {
        let mut output = Output::new(budget, &mut send);
        let result = self
            .check_time(now)
            .and_then(|()| self.tick_inner(local, now, &mut output));
        let events = self.finish(output.report, result)?;
        Ok(TickOutput {
            events,
            report: self.last_report,
        })
    }
    fn expire(&mut self, now: Duration, output: &mut Output<'_>) {
        self.peers.retain(|p| {
            if now - p.last_seen > PEER_TIMEOUT {
                self.game.remove_player(p.id);
                output.report.peers_timed_out += 1;
                false
            } else {
                true
            }
        });
    }
    fn tick_inner(
        &mut self,
        local: &[(u8, InputFrame)],
        now: Duration,
        output: &mut Output<'_>,
    ) -> Result<Vec<crate::GameEvent>, DriverError> {
        self.validate_local(local)?;
        self.expire(now, output);
        self.drain_commands(output)?;
        self.prepare_tactical()?;
        let mut commands = [(0, InputFrame::default()); MAX_PLAYERS];
        let mut command_count = 0;
        let mut shots = [crate::ShotTime { player: 0, tick: 0 }; MAX_PLAYERS];
        let mut shot_count = 0;
        for peer in &mut self.peers {
            let queued = peer.inputs.len();
            if let Some(timed) = peer.inputs.pop_latest_timed() {
                output.report.inputs_consumed += 1;
                output.report.inputs_coalesced += queued - 1;
                let input = timed.input;
                peer.last_input = input;
                peer.last_shot = timed.shot_tick;
                peer.last_input_tick = self.game.tick;
                peer.ack = Some(input.sequence);
            }
            let mut input = peer.last_input;
            // Do not leave movement/fire held forever when packets stop arriving.
            if self.game.tick.wrapping_sub(peer.last_input_tick) > 12 {
                input.move_x = 0.;
                input.move_z = 0.;
                input.buttons = 0;
                if let Some(mode) = &mut self.tactical
                    && mode
                        .rules
                        .interaction_progress()
                        .is_some_and(|(id, _)| id == u16::from(peer.id))
                {
                    let _ = mode
                        .rules
                        .act(u16::from(peer.id), ferravex_play::rounds::Action::Cancel);
                    self.round_dirty = true;
                }
            }
            if input.buttons & crate::BUTTON_FIRE != 0
                && let Some(tick) = peer.last_shot
            {
                shots[shot_count] = crate::ShotTime {
                    player: peer.id,
                    tick,
                };
                shot_count += 1;
            }
            commands[command_count] = (peer.id, input);
            command_count += 1;
        }
        for &(id, input) in local {
            if command_count == MAX_PLAYERS {
                return Err(DriverError::InvalidLocalInputs);
            }
            commands[command_count] = (id, input);
            command_count += 1;
        }
        output.report.authority_inputs = command_count;
        let step = self.step_combat(&commands[..command_count], &shots[..shot_count]);
        self.rejected_shots = self
            .rejected_shots
            .saturating_add(step.rejected_shots.len() as u64);
        for (id, _) in &step.rejected_shots {
            if let Some(peer) = self.peers.iter_mut().find(|p| p.id == *id) {
                peer.last_input.buttons &= !crate::BUTTON_FIRE;
                peer.last_shot = None;
            }
        }

        output.report.tick_executed = true;
        let events = step.events;
        let mut payloads: Vec<_> = events
            .iter()
            .map(|event| {
                NetworkEvent {
                    tick: self.game.tick,
                    event: *event,
                }
                .encode()
            })
            .collect::<Result<_, _>>()
            .map_err(DriverError::Protocol)?;
        if let Some(payload) = self.round_payload()? {
            payloads.push(payload);
        }
        let mut slow = [false; MAX_PLAYERS];
        for peer in &mut self.peers {
            for payload in &payloads {
                if peer.events.pending()
                    + peer.commands.buffered()
                    + usize::from(peer.pending_result.is_some())
                    >= ferravex_net::reliable::EVENT_LIMIT
                    || peer.events.queue(payload).is_err()
                {
                    slow[peer.id as usize] = true;
                    break;
                }
            }
        }
        // Reliability is not silently sacrificed when a consumer stops ACKing.
        // Bound per-peer state and require a fresh session for an overflowing peer.
        self.peers.retain(|peer| {
            if slow[peer.id as usize] {
                self.slow_event_disconnects += 1;
                self.game.remove_player(peer.id);
                false
            } else {
                true
            }
        });
        if self.round_dirty || self.game.tick.is_multiple_of(SNAPSHOT_INTERVAL) {
            for peer in &mut self.peers {
                Self::send_snapshot(&self.game, peer, output)?;
            }
        }
        self.round_dirty = false;
        self.flush_events(now, output)?;
        Ok(events)
    }
}
impl ServerState {
    pub(super) fn validate_local(&self, local: &[(u8, InputFrame)]) -> Result<(), DriverError> {
        if local.len() > MAX_PLAYERS
            || local.iter().enumerate().any(|(i, (id, _))| {
                self.game.player(*id).is_none()
                    || local[..i].iter().any(|(old, _)| old == id)
                    || self.peers.iter().any(|p| p.id == *id)
            })
        {
            return Err(DriverError::InvalidLocalInputs);
        }
        Ok(())
    }
    pub(super) fn admit_input(
        peer: &mut Peer,
        input: ferravex_net::TimedInput,
        output: &mut Output<'_>,
    ) {
        match peer.inputs.push_timed_checked(input) {
            Ok(()) => output.report.inputs_accepted += 1,
            Err(e) => {
                output.report.inputs_refused += 1;
                match e {
                    InputAdmissionError::InvalidInput => output.report.inputs_invalid += 1,
                    InputAdmissionError::Full => output.report.inputs_full += 1,
                    InputAdmissionError::DuplicateNewest => output.report.inputs_duplicate += 1,
                    InputAdmissionError::OutOfOrder => output.report.inputs_out_of_order += 1,
                }
            }
        }
    }
    fn receive_packet(
        &mut self,
        addr: SocketAddr,
        packet: ClientPacket,
        now: Duration,
        cookie: &mut impl FnMut() -> u64,
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
        match packet {
            ClientPacket::Hello { nonce } => {
                if let Some(index) = self.peers.iter().position(|p| p.addr == addr) {
                    let peer = &mut self.peers[index];
                    if now - peer.last_hello < HELLO_INTERVAL {
                        return Ok(());
                    }
                    if peer.nonce != nonce {
                        let session = cookie();
                        if session == 0 {
                            return Err(DriverError::InvalidCookie);
                        }
                        peer.nonce = nonce;
                        peer.session = session;
                        peer.inputs.reset_session();
                        peer.events = EventSender::default();
                        peer.commands = EventReceiver::default();
                        peer.pending_result = None;
                        self.round_dirty = true;
                        peer.last_input = InputFrame::default();
                        peer.last_shot = None;
                        peer.snapshots.reset();
                        peer.last_ping = None;
                        peer.ack = None;
                        peer.last_input_tick = self.game.tick;
                        if let Some(player) = self.game.player(peer.id) {
                            peer.last_input.yaw = player.yaw;
                            peer.last_input.pitch = player.pitch;
                        }
                    }
                    peer.last_hello = now;
                    peer.last_seen = now;
                } else {
                    if self.peers.len() >= MAX_PLAYERS || !self.may_join() {
                        output.report.packets_rejected += 1;
                        return Ok(());
                    }
                    let Some(id) =
                        (0..MAX_PLAYERS as u8).find(|id| self.game.player(*id).is_none())
                    else {
                        output.report.packets_rejected += 1;
                        return Ok(());
                    };
                    let session = cookie();
                    if session == 0 {
                        return Err(DriverError::InvalidCookie);
                    }
                    // Allocate/validate bounded retention before spawning an actor.
                    let snapshots = SenderBaselines::new(BaselineLimits {
                        max_snapshots: 32,
                        max_bytes: 32 * MAX_PACKET_BYTES,
                    })?;
                    if self
                        .tactical
                        .as_ref()
                        .is_some_and(|m| m.rules.phase() == ferravex_play::rounds::Phase::Finished)
                    {
                        self.tactical = None;
                        self.round_dirty = true;
                    }
                    self.round_dirty |= self.tactical_options.is_some();
                    self.game.spawn_player(id);
                    self.peers.push(Peer {
                        addr,
                        nonce,
                        session,
                        id,
                        inputs: InputQueue::default(),
                        last_input: InputFrame::default(),
                        last_shot: None,
                        snapshots,
                        delta: Delta {
                            base_tick: 0,
                            tick: 0,
                            bytes: Vec::with_capacity(MAX_PACKET_BYTES),
                        },
                        last_ping: None,
                        commands: EventReceiver::default(),
                        pending_result: None,
                        last_input_tick: self.game.tick,
                        ack: None,
                        last_seen: now,
                        last_hello: now,
                        events: EventSender::default(),
                    });
                }
                let peer = self
                    .peers
                    .iter_mut()
                    .find(|p| p.addr == addr)
                    .expect("admitted peer");
                output.packet(
                    addr,
                    &ServerPacket::Welcome {
                        nonce: peer.nonce,
                        session: peer.session,
                        player_id: peer.id,
                        tick: self.game.tick,
                    },
                )?;
                Self::send_snapshot(&self.game, peer, output)?;
            }
            ClientPacket::EventAck { session, sequence } => {
                let Some(peer) = self
                    .peers
                    .iter_mut()
                    .find(|p| p.addr == addr && p.session == session)
                else {
                    output.report.packets_rejected += 1;
                    return Ok(());
                };
                if peer.events.acknowledge(sequence) {
                    peer.last_seen = now;
                }
            }
            ClientPacket::Input { session, inputs } => {
                let Some(peer) = self
                    .peers
                    .iter_mut()
                    .find(|p| p.addr == addr && p.session == session)
                else {
                    output.report.packets_rejected += 1;
                    return Ok(());
                };
                peer.last_seen = now;
                for input in inputs {
                    Self::admit_input(
                        peer,
                        ferravex_net::TimedInput {
                            input,
                            shot_tick: None,
                        },
                        output,
                    );
                }
            }
            ClientPacket::Extension(packet) => self.extension(addr, packet, now, output)?,
        }
        Ok(())
    }
    fn flush_events(&mut self, now: Duration, output: &mut Output<'_>) -> Result<(), DriverError> {
        let millis = now.as_millis() as u64;
        for peer in &mut self.peers {
            let mut error = None;
            let mut bytes = [0; MAX_PACKET_BYTES];
            let report = peer.events.send_due(
                millis,
                EVENT_SEND_BUDGET,
                EVENT_SEND_BUDGET * ferravex_net::reliable::EVENT_BYTES,
                |event| match (BorrowedEvent::ReliableEvent {
                    session: peer.session,
                    event,
                })
                .encode_into(&mut bytes)
                {
                    Ok(n) => output.offer(peer.addr, &bytes[..n]),
                    Err(e) => {
                        error = Some(e);
                        false
                    }
                },
            );
            output.report.reliable_remaining_due += report.remaining_due;
            if let Some(e) = error {
                return Err(DriverError::Encode(e));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_net::{
        ClientExtension, ServerExtension, decode_server, encode_client, encoded_server_len,
    };
    fn addr(n: u16) -> SocketAddr {
        ([127, 0, 0, 1], n).into()
    }
    fn receive(
        state: &mut ServerState,
        packet: ClientPacket,
        now: Duration,
        budget: OutputBudget,
        send: impl FnMut(SocketAddr, &[u8]) -> bool,
    ) -> Result<WorkReport, DriverError> {
        state.receive(
            addr(9000),
            &encode_client(&packet).unwrap(),
            now,
            || 7,
            budget,
            send,
        )
    }
    fn hello() -> ClientPacket {
        ClientPacket::Hello { nonce: 1 }
    }
    #[test]
    fn output_zero_short_exact_prefix_and_no_ack_authority_for_deferred_snapshot() {
        let mut state = ServerState::new(Arena::default());
        let report = receive(
            &mut state,
            hello(),
            Duration::ZERO,
            OutputBudget {
                datagrams: 0,
                bytes: 0,
            },
            |_, _| panic!("zero output budget"),
        )
        .unwrap();
        assert_eq!(report.datagrams_accepted, 0);
        assert_eq!(report.datagrams_refused, 0);
        assert_eq!(report.datagrams_deferred, 2);
        assert_eq!(state.peer_count(), 1);
        assert!(state.peers[0].snapshots.is_empty());
        let welcome_len = encoded_server_len(&ServerPacket::Welcome {
            nonce: 1,
            session: 7,
            player_id: 0,
            tick: 0,
        })
        .unwrap();
        let report = receive(
            &mut state,
            hello(),
            HELLO_INTERVAL,
            OutputBudget {
                datagrams: 9,
                bytes: welcome_len - 1,
            },
            |_, _| panic!("short byte budget"),
        )
        .unwrap();
        assert_eq!(report.datagrams_deferred, 2);
        let mut offered = Vec::new();
        let report = receive(
            &mut state,
            hello(),
            HELLO_INTERVAL * 2,
            OutputBudget {
                datagrams: 1,
                bytes: welcome_len,
            },
            |_, bytes| {
                offered.push(bytes.to_vec());
                true
            },
        )
        .unwrap();
        assert_eq!(report.datagrams_accepted, 1);
        assert_eq!(report.bytes_accepted, welcome_len);
        assert_eq!(report.datagrams_deferred, 1);
        assert!(matches!(
            decode_server(&offered[0]).unwrap(),
            ServerPacket::Welcome { .. }
        ));
        let report = receive(
            &mut state,
            ClientPacket::Extension(ClientExtension::SnapshotAck {
                session: 7,
                tick: Some(0),
            }),
            HELLO_INTERVAL * 2,
            OutputBudget::default(),
            |_, _| true,
        )
        .unwrap();
        assert_eq!(report.packets_rejected, 1);
        let report = receive(
            &mut state,
            hello(),
            HELLO_INTERVAL * 3,
            OutputBudget::default(),
            |_, bytes| {
                // The initialized prefix must decode exactly; no scratch tail leaks.
                decode_server(bytes).unwrap();
                true
            },
        )
        .unwrap();
        assert_eq!(report.datagrams_accepted, 2);
        assert_eq!(state.peers[0].snapshots.len(), 1);
    }
    #[test]
    fn reliable_accepted_prefix_refused_head_untouched_tail_and_byte_limit() {
        let mut state = ServerState::new(Arena::default());
        receive(
            &mut state,
            hello(),
            Duration::ZERO,
            OutputBudget::default(),
            |_, _| true,
        )
        .unwrap();
        for payload in [b"first".as_slice(), b"second", b"third"] {
            state.peers[0].events.queue(payload).unwrap();
        }
        let mut offered = Vec::new();
        let report = state
            .poll(Duration::ZERO, OutputBudget::default(), |_, bytes| {
                let ServerPacket::ReliableEvent { event, .. } = decode_server(bytes).unwrap()
                else {
                    panic!("event")
                };
                offered.push(event.sequence);
                event.sequence == 0
            })
            .unwrap();
        assert_eq!(offered, [0, 1]);
        assert_eq!(report.datagrams_accepted, 1);
        assert_eq!(report.datagrams_refused, 1);
        assert_eq!(report.reliable_remaining_due, 2);
        let mut retried = Vec::new();
        let report = state
            .poll(
                Duration::ZERO,
                OutputBudget {
                    datagrams: 99,
                    bytes: 24 + b"second".len(),
                },
                |_, bytes| {
                    let ServerPacket::ReliableEvent { event, .. } = decode_server(bytes).unwrap()
                    else {
                        panic!("event")
                    };
                    retried.push(event.sequence);
                    true
                },
            )
            .unwrap();
        assert_eq!(retried, [1]);
        assert_eq!(report.datagrams_accepted, 1);
        assert_eq!(report.datagrams_deferred, 1);
        assert_eq!(report.reliable_remaining_due, 1);
        let report = state.poll(Duration::ZERO, OutputBudget::default(), |_,bytes| {
            assert!(matches!(decode_server(bytes).unwrap(), ServerPacket::ReliableEvent { event, .. } if event.sequence == 2)); true
        }).unwrap();
        assert_eq!(report.datagrams_accepted, 1);
        assert_eq!(report.reliable_remaining_due, 0);
        assert_eq!(
            state.pending_event_count(),
            3,
            "local admission never acts as remote ACK"
        );
    }
    #[test]
    fn poll_caps_all_peer_outgoing_work_and_peer_admission() {
        let mut state = ServerState::new(Arena::default());
        let wire = encode_client(&hello()).unwrap();
        for n in 0..MAX_PLAYERS as u16 {
            state
                .receive(
                    addr(9000 + n),
                    &wire,
                    Duration::ZERO,
                    || u64::from(n) + 1,
                    OutputBudget::default(),
                    |_, _| true,
                )
                .unwrap();
            for _ in 0..10 {
                state.peers[n as usize].events.queue(b"test").unwrap();
            }
        }
        let report = state
            .receive(
                addr(9999),
                &wire,
                Duration::ZERO,
                || panic!("full roster"),
                OutputBudget::default(),
                |_, _| panic!("full roster"),
            )
            .unwrap();
        assert_eq!(report.packets_rejected, 1);
        let report = state
            .poll(
                Duration::ZERO,
                OutputBudget {
                    datagrams: usize::MAX,
                    bytes: usize::MAX,
                },
                |_, _| true,
            )
            .unwrap();
        assert_eq!(report.datagrams_accepted, 128);
        assert_eq!(report.reliable_remaining_due, 32);
        assert_eq!(state.peer_count(), MAX_PLAYERS);
        assert_eq!(state.pending_event_count(), 160);
    }
    #[test]
    fn refused_pong_remains_eligible_but_accepted_ping_rate_gate_is_preserved() {
        let mut state = ServerState::new(Arena::default());
        receive(
            &mut state,
            hello(),
            Duration::ZERO,
            OutputBudget::default(),
            |_, _| true,
        )
        .unwrap();
        let ping = || {
            ClientPacket::Extension(ClientExtension::Ping {
                session: 7,
                nonce: 9,
                client_us: 0,
            })
        };
        let report = receive(
            &mut state,
            ping(),
            Duration::ZERO,
            OutputBudget::default(),
            |_, _| false,
        )
        .unwrap();
        assert_eq!(report.datagrams_refused, 1);
        let report = receive(
            &mut state,
            ping(),
            Duration::ZERO,
            OutputBudget::default(),
            |_, bytes| {
                assert!(matches!(
                    decode_server(bytes).unwrap(),
                    ServerPacket::Extension(ServerExtension::Pong {
                        receive_us: 0,
                        send_us: 0,
                        ..
                    })
                ));
                true
            },
        )
        .unwrap();
        assert_eq!(report.datagrams_accepted, 1);
        let report = receive(
            &mut state,
            ping(),
            Duration::from_millis(99),
            OutputBudget::default(),
            |_, _| panic!("rate gate"),
        )
        .unwrap();
        assert_eq!(report.datagrams_accepted, 0);
        let report = receive(
            &mut state,
            ping(),
            Duration::from_millis(100),
            OutputBudget::default(),
            |_, _| true,
        )
        .unwrap();
        assert_eq!(report.datagrams_accepted, 1);
    }
    #[test]
    fn result_capacity_is_preflighted_and_failed_queue_keeps_durable_result() {
        let mut state = ServerState::new(Arena::default());
        receive(
            &mut state,
            hello(),
            Duration::ZERO,
            OutputBudget::default(),
            |_, _| true,
        )
        .unwrap();
        for _ in 0..ferravex_net::reliable::EVENT_LIMIT {
            state.peers[0].events.queue(b"full").unwrap();
        }
        // This is a deliberately retained completed result, not a fake action.
        // A failure cannot drop it when the sender window is full.
        state.peers[0].pending_result = Some(crate::tactical::SessionEvent::ActionResult {
            tick: 0,
            sequence: 0,
            result: Err(ferravex_play::rounds::RuleError::WrongPhase),
        });
        assert!(matches!(
            state.poll(Duration::ZERO, OutputBudget::default(), |_, _| true),
            Err(DriverError::Reliable(ferravex_net::reliable::Error::Full))
        ));
        assert!(state.peers[0].pending_result.is_some());
        assert_eq!(state.last_report().commands_executed, 0);
        state.peers[0].events.acknowledge(0);
        state
            .poll(Duration::ZERO, OutputBudget::default(), |_, _| true)
            .unwrap();
        assert!(state.peers[0].pending_result.is_none());
        assert_eq!(
            state.peers[0].events.pending(),
            ferravex_net::reliable::EVENT_LIMIT
        );
    }
    #[test]
    fn partial_error_retains_accepted_callbacks_and_distinguishes_executed_tick() {
        let mut state = ServerState::new(Arena::default());
        receive(
            &mut state,
            hello(),
            Duration::ZERO,
            OutputBudget::default(),
            |_, _| true,
        )
        .unwrap();
        // Public raw Match access historically allows invalid fields. A later
        // encoder error does not undo an already accepted Welcome or tick.
        state.game.player_mut(0).unwrap().health = 101;
        let result = receive(
            &mut state,
            hello(),
            HELLO_INTERVAL,
            OutputBudget::default(),
            |_, bytes| {
                assert!(matches!(
                    decode_server(bytes).unwrap(),
                    ServerPacket::Welcome { .. }
                ));
                true
            },
        );
        assert!(matches!(result, Err(DriverError::Encode(_))));
        assert_eq!(state.last_report().datagrams_accepted, 1);
        assert_eq!(state.last_report().errors, 1);
        assert!(!state.last_report().tick_executed);
        state
            .tick(&[], HELLO_INTERVAL, OutputBudget::default(), |_, _| true)
            .unwrap();
        state
            .tick(&[], HELLO_INTERVAL, OutputBudget::default(), |_, _| true)
            .unwrap();
        assert!(matches!(
            state.tick(&[], HELLO_INTERVAL, OutputBudget::default(), |_, _| true),
            Err(DriverError::Encode(_))
        ));
        assert_eq!(state.game.tick, 3);
        assert!(state.last_report().tick_executed);
        assert_eq!(state.last_report().errors, 1);
    }
}
