//! Transport-independent production prediction client for protocol v5.
//!
//! Supply one monotonic microsecond time domain (0..=2^53 us). `receive` handles
//! exactly one datagram; `poll` maintains the protocol through render/menu frames;
//! only `tick` executes a new prediction step. Reconciliation replays at most 128
//! retained inputs through the same practice/tactical predictor as the adapter.
//! A callback returns true only for complete local datagram admission. Refusal
//! stops output for that operation, retaining retries/tails; prior effects persist.
//! No transport, clock read, entropy source or frame loop lives in this state.
//!
//! A timeout clears session authority immediately and sets `needs_reconnect`.
//! The host must call `reconnect` with a fresh, nonzero, never-reused nonce before
//! another Hello. Keep the same time domain across reconnects. Nonces/cookies are
//! accidental stale-traffic protection, not authentication or encryption.
//!
//! ```
//! use ferravex_fps::{Arena, InputFrame, client_state::ClientState, server_state::ServerState};
//! use std::{collections::VecDeque, net::SocketAddr, time::Duration};
//! let peer: SocketAddr = ([127, 0, 0, 1], 7).into();
//! let mut server = ServerState::new(Arena::default());
//! let mut client = ClientState::new(Arena::default(), 17)?;
//! // These host mailboxes explicitly admit bounded whole datagrams.
//! let mut upstream = VecDeque::new();
//! let mut downstream = VecDeque::new();
//! let now = Duration::ZERO;
//! client.poll(now, Default::default(), |bytes| {
//!     if upstream.len() == 8 { return false; }
//!     upstream.push_back(bytes.to_vec()); true
//! })?;
//! while let Some(bytes) = upstream.pop_front() {
//!     server.receive(peer, &bytes, now, || 23, Default::default(), |to, bytes| {
//!         assert_eq!(to, peer);
//!         if downstream.len() == 8 { return false; }
//!         downstream.push_back(bytes.to_vec()); true
//!     })?;
//! }
//! while let Some(bytes) = downstream.pop_front() {
//!     client.receive(&bytes, now, Default::default(), |_| true)?;
//! }
//! assert!(client.is_connected());
//! let position = client.local_player().unwrap().position;
//! client.poll(Duration::from_millis(10), Default::default(), |_| false)?;
//! assert_eq!(client.local_player().unwrap().position, position); // No implicit tick.
//! let report = client.tick(InputFrame { move_z: 1., ..Default::default() },
//!                          Duration::from_millis(16), Default::default(), |_| false)?;
//! assert!(report.tick_executed && report.inputs_retained == 1);
//! assert_eq!(client.pending_input_count(), 1); // Retained even on output refusal.
//! let _poses = client.rendered_players(Duration::from_millis(16))?;
//! client.reconnect(18, Duration::from_millis(17))?;
//! assert!(!client.is_connected());
//! assert_eq!(client.pending_input_count(), 0);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use super::tactical::predict;
use super::{
    HELLO_INTERVAL, MAX_DELIVERED_EVENTS, MAX_PENDING_INPUTS, PEER_TIMEOUT, POLL_BUDGET,
    RenderFrame,
};
use crate::tactical::{SessionEvent, View, decode_action, encode_action};
use crate::{Arena, InputFrame, MAX_PLAYERS, NetworkEvent, Player};
use ferravex_net::baselines::{BaselineError, BaselineLimits, ReceiverBaselines};
use ferravex_net::clock::{ClientClock, ClockDiagnostics, TickUnwrapper};
use ferravex_net::reliable::{self, EventReceiver, EventSender, Receipt};
use ferravex_net::{
    BorrowedEvent, ClientExtension as CX, ClientPacket, EncodeError, MAX_INPUTS_PER_PACKET,
    MAX_PACKET_BYTES, ServerExtension as SX, ServerPacket, Snapshot, TimedInput, decode_server,
    encode_client_into, sequence_newer,
};
use ferravex_play::rounds::{Action, RuleError};
use std::{collections::VecDeque, time::Duration};

/// Whole-envelope callback limits, shared with the authority; zero is valid.
pub use super::server_state::OutputBudget;
const BASELINES: usize = 32;
const EVENT_WORK: usize = 8;

/// Per-operation work, including committed work before an error. Deferred counts
/// generated offers, not undiscovered tails. Retained counts are end-of-call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkReport {
    pub datagrams_accepted: usize,
    pub datagrams_refused: usize,
    pub datagrams_deferred: usize,
    pub bytes_accepted: usize,
    pub packets_received: usize,
    pub packets_rejected: usize,
    pub snapshots_installed: usize,
    pub inputs_retained: usize,
    pub inputs_evicted: usize,
    pub inputs_acknowledged: usize,
    pub prediction_steps: usize,
    pub replayed_inputs: usize,
    pub events_retained: usize,
    pub events_duplicate: usize,
    pub events_refused: usize,
    pub events_consumed: usize,
    pub events_buffered: usize,
    pub events_ready: usize,
    pub events_delivered: usize,
    pub acknowledgements_pending: usize,
    pub commands_pending: usize,
    pub commands_remaining_due: usize,
    pub tick_executed: bool,
    pub reconnect_required: bool,
    pub errors: usize,
}

/// Errors never retract an executed prediction/reconciliation or admitted output.
/// Inspect `last_report` before retrying a tick. Malformed/stale ingress is counted
/// as packet rejection, not a repeating driver error.
#[derive(Debug)]
#[non_exhaustive]
pub enum DriverError {
    InvalidTime,
    BackwardsTime,
    InvalidNonce,
    NotConnected,
    Encode(EncodeError),
    Baseline(BaselineError),
    Reliable(reliable::Error),
    Protocol(ferravex_net::ProtocolError),
}
impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "client driver: {self:?}")
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

struct Output<'a> {
    report: WorkReport,
    budget: OutputBudget,
    stopped: bool,
    send: &'a mut dyn FnMut(&[u8]) -> bool,
}
impl<'a> Output<'a> {
    fn new(budget: OutputBudget, send: &'a mut dyn FnMut(&[u8]) -> bool) -> Self {
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
    fn offer(&mut self, bytes: &[u8]) -> bool {
        if self.stopped
            || self.report.datagrams_accepted == self.budget.datagrams
            || bytes.len() > self.budget.bytes - self.report.bytes_accepted
        {
            self.stopped = true;
            self.report.datagrams_deferred += 1;
            return false;
        }
        if !(self.send)(bytes) {
            self.stopped = true;
            self.report.datagrams_refused += 1;
            return false;
        }
        self.report.datagrams_accepted += 1;
        self.report.bytes_accepted += bytes.len();
        true
    }
    fn packet(&mut self, packet: &ClientPacket) -> Result<bool, DriverError> {
        let mut bytes = [0; MAX_PACKET_BYTES];
        let n = encode_client_into(packet, &mut bytes)?;
        Ok(self.offer(&bytes[..n]))
    }
}

/// One bounded prediction, replication, clock and reliable-event state. Public
/// counter field access remains available through the GameClient adapter's Deref.
/// 128 newest inputs; 32 full baselines / 38400 bytes; 32 render frames; <=512
/// combined queued gameplay events and reliable receipts, <=256 receipts; <=256
/// pending event ACKs; <=256 commands. See module docs and `WorkReport`.
pub struct ClientState {
    pub(super) arena: Arena,
    pub(super) nonce: u64,
    session_nonce: Option<u64>,
    pub(super) session: Option<u64>,
    pub(super) id: Option<u8>,
    pub(super) next_sequence: u32,
    pub(super) pending: VecDeque<TimedInput>,
    pub(super) snapshots: ReceiverBaselines,
    clock: ClientClock,
    unwrapper: TickUnwrapper,
    history: ferravex_net::history::History<RenderFrame>,
    last_ping: Option<Duration>,
    last_full_request: Option<Duration>,
    pub(super) predicted: Option<Player>,
    pub(super) commands: EventSender,
    round: Option<View>,
    lobby: Option<(u8, u8)>,
    action_result: Option<(u64, Result<(), RuleError>)>,
    players: Vec<Player>,
    previous_players: Vec<Player>,
    epochs: [u32; MAX_PLAYERS],
    previous_epochs: [u32; MAX_PLAYERS],
    pub(super) latest_tick: Option<u32>,
    previous_tick: u32,
    last_snapshot: Option<Duration>,
    last_hello: Option<Duration>,
    ack: Option<u32>,
    pub rejected_packets: u64,
    pub(super) event_receiver: EventReceiver,
    pub(super) delivered_events: VecDeque<NetworkEvent>,
    event_acks: VecDeque<u64>,
    snapshot_ack: Option<u32>,
    full_requested: bool,
    input_dirty: bool,
    reconnect_required: bool,
    now: Duration,
    pub(super) last_report: WorkReport,
}
impl ClientState {
    /// Construct without sending. Host supplies a fresh nonzero nonce, then polls.
    pub fn new(arena: Arena, nonce: u64) -> Result<Self, DriverError> {
        if nonce == 0 {
            return Err(DriverError::InvalidNonce);
        }
        Ok(Self {
            arena,
            nonce,
            session_nonce: None,
            session: None,
            id: None,
            next_sequence: 0,
            pending: VecDeque::with_capacity(MAX_PENDING_INPUTS),
            snapshots: ReceiverBaselines::new(BaselineLimits {
                max_snapshots: BASELINES,
                max_bytes: BASELINES * MAX_PACKET_BYTES,
            })?,
            clock: ClientClock::default(),
            unwrapper: TickUnwrapper::default(),
            history: ferravex_net::history::History::new(BASELINES),
            last_ping: None,
            last_full_request: None,
            predicted: None,
            commands: EventSender::default(),
            round: None,
            lobby: None,
            action_result: None,
            players: vec![],
            previous_players: vec![],
            epochs: [0; MAX_PLAYERS],
            previous_epochs: [0; MAX_PLAYERS],
            latest_tick: None,
            previous_tick: 0,
            last_snapshot: None,
            last_hello: None,
            ack: None,
            rejected_packets: 0,
            event_receiver: EventReceiver::default(),
            delivered_events: VecDeque::new(),
            event_acks: VecDeque::new(),
            snapshot_ack: None,
            full_requested: false,
            input_dirty: false,
            reconnect_required: false,
            now: Duration::ZERO,
            last_report: WorkReport::default(),
        })
    }
    fn validate_time(&self, now: Duration) -> Result<(), DriverError> {
        if now > Duration::from_micros(1 << 53) || !now.subsec_nanos().is_multiple_of(1000) {
            return Err(DriverError::InvalidTime);
        }
        if now < self.now {
            return Err(DriverError::BackwardsTime);
        }
        Ok(())
    }
    fn advance_time(&mut self, now: Duration) -> Result<(), DriverError> {
        self.validate_time(now)?;
        self.now = now;
        Ok(())
    }
    fn clear_session(&mut self) {
        self.session = None;
        self.session_nonce = None;
        self.id = None;
        self.predicted = None;
        self.next_sequence = 0;
        self.pending.clear();
        self.snapshots.reset();
        self.clock.reset();
        self.unwrapper.reset();
        self.history.clear();
        self.last_ping = None;
        self.last_full_request = None;
        self.commands = EventSender::default();
        self.round = None;
        self.lobby = None;
        self.action_result = None;
        self.players.clear();
        self.previous_players.clear();
        self.epochs = [0; MAX_PLAYERS];
        self.previous_epochs = [0; MAX_PLAYERS];
        self.latest_tick = None;
        self.previous_tick = 0;
        self.last_snapshot = None;
        self.last_hello = None;
        self.ack = None;
        self.event_receiver = EventReceiver::default();
        self.delivered_events.clear();
        self.event_acks.clear();
        self.snapshot_ack = None;
        self.full_requested = false;
        self.input_dirty = false;
    }
    /// Atomically discard session authority, held edges/predictions, histories,
    /// probes and queues. Nonzero nonce must differ from the previous one; the
    /// host must additionally prevent reuse of any older nonce. No output/tick.
    pub fn reconnect(&mut self, nonce: u64, now: Duration) -> Result<(), DriverError> {
        let result = self.validate_time(now).and_then(|()| {
            if nonce == 0 || nonce == self.nonce {
                return Err(DriverError::InvalidNonce);
            }
            self.now = now;
            self.clear_session();
            self.nonce = nonce;
            self.reconnect_required = false;
            Ok(())
        });
        self.finish(WorkReport::default(), result)?;
        Ok(())
    }
    pub fn needs_reconnect(&self) -> bool {
        self.reconnect_required
    }
    pub fn last_report(&self) -> WorkReport {
        self.last_report
    }
    pub fn player_id(&self) -> Option<u8> {
        self.id
    }
    pub fn is_connected(&self) -> bool {
        self.id.is_some() && self.predicted.is_some()
    }
    pub fn local_player(&self) -> Option<&Player> {
        self.predicted.as_ref()
    }
    pub fn players(&self) -> &[Player] {
        &self.players
    }
    pub fn server_tick(&self) -> u32 {
        self.latest_tick.unwrap_or(0)
    }
    /// Transfer final gameplay delivery. Retained ordered tails drain on a later
    /// poll/receive/tick, even when no new packet arrives. Call poll after draining.
    pub fn drain_events(&mut self) -> Vec<NetworkEvent> {
        self.delivered_events.drain(..).collect()
    }
    pub fn pending_event_count(&self) -> usize {
        self.delivered_events.len()
    }
    pub fn retained_event_count(&self) -> usize {
        self.event_receiver.buffered()
    }
    pub fn pending_input_count(&self) -> usize {
        self.pending.len()
    }
    pub fn acknowledged_sequence(&self) -> Option<u32> {
        self.ack
    }
    pub fn retained_baseline_count(&self) -> usize {
        self.snapshots.len()
    }
    pub fn retained_baseline_bytes(&self) -> usize {
        self.snapshots.retained_bytes()
    }
    pub fn snapshot_age(&self, now: Duration) -> Result<Option<Duration>, DriverError> {
        self.validate_time(now)?;
        Ok(self.last_snapshot.map(|t| now - t))
    }
    pub fn clock_diagnostics(&self, now: Duration) -> Result<ClockDiagnostics, DriverError> {
        self.validate_time(now)?;
        Ok(self
            .clock
            .diagnostics(now.as_micros() as u64, crate::TICK_RATE as u16))
    }
    pub fn tactical_lobby(&self) -> Option<(u8, u8)> {
        self.lobby
    }
    pub fn tactical_view(&self) -> Option<&View> {
        self.round.as_ref()
    }
    pub fn last_action_result(&self) -> Option<(u64, Result<(), RuleError>)> {
        self.action_result
    }
    pub fn pending_command_count(&self) -> usize {
        self.commands.pending()
    }
    /// Queue validated application bytes only in a tactical session. Full and
    /// sequence exhaustion are explicit reliable errors and do not apply actions.
    pub fn queue_action(&mut self, action: Action) -> Result<u64, DriverError> {
        if self.session.is_none() || self.round.is_none() {
            return Err(DriverError::NotConnected);
        }
        let bytes = encode_action(action);
        decode_action(&bytes).map_err(DriverError::Protocol)?;
        self.commands.queue(&bytes).map_err(DriverError::Reliable)
    }
    fn finish<T>(
        &mut self,
        mut report: WorkReport,
        result: Result<T, DriverError>,
    ) -> Result<T, DriverError> {
        if result.is_err() {
            report.errors += 1;
        }
        report.events_buffered = self.event_receiver.buffered();
        report.events_delivered = self.delivered_events.len();
        report.inputs_retained = self.pending.len();
        report.acknowledgements_pending = self.event_acks.len()
            + usize::from(self.snapshot_ack.is_some())
            + usize::from(self.full_requested);
        report.commands_pending = self.commands.pending();
        report.reconnect_required = self.reconnect_required;
        self.rejected_packets = self
            .rejected_packets
            .saturating_add(report.packets_rejected as u64);
        self.last_report = report;
        result
    }
    fn expire(&mut self) {
        if self
            .last_snapshot
            .is_some_and(|t| self.now - t > PEER_TIMEOUT)
        {
            self.clear_session();
            self.reconnect_required = true;
        }
    }
    /// Decode and admit one datagram. Session/application checks precede durable
    /// retention. Snapshot publication/ACK follows exact full-byte installation.
    pub fn receive(
        &mut self,
        bytes: &[u8],
        now: Duration,
        budget: OutputBudget,
        mut send: impl FnMut(&[u8]) -> bool,
    ) -> Result<WorkReport, DriverError> {
        let mut output = Output::new(budget, &mut send);
        let result = self.advance_time(now).and_then(|()| {
            self.expire();
            output.report.packets_received = 1;
            match decode_server(bytes) {
                Ok(packet) => self.receive_packet(packet, bytes, &mut output)?,
                Err(_) => output.report.packets_rejected += 1,
            }
            self.drain_received(&mut output.report);
            self.flush_acknowledgements(&mut output)
        });
        self.finish(output.report, result)?;
        Ok(self.last_report)
    }
    /// Protocol maintenance only. A timed-out session requires a fresh host nonce.
    /// At most eight ordered event applications and eight command sends per call.
    pub fn poll(
        &mut self,
        now: Duration,
        budget: OutputBudget,
        mut send: impl FnMut(&[u8]) -> bool,
    ) -> Result<WorkReport, DriverError> {
        let mut output = Output::new(budget, &mut send);
        let result = self.advance_time(now).and_then(|()| {
            self.expire();
            self.drain_received(&mut output.report);
            self.flush_acknowledgements(&mut output)?;
            self.hello(&mut output)?;
            self.flush_input(&mut output)?;
            self.flush_commands(&mut output)?;
            self.probe_clock(&mut output)
        });
        self.finish(output.report, result)?;
        Ok(self.last_report)
    }
    /// Execute one fixed prediction tick; call only for actual executed ticks.
    /// Newest-128 retention preserves legacy overload policy and reports each
    /// evicted oldest input. The latest six form a retryable outbound tail.
    /// There is no receive loop or catch-up loop. Refusal still completes the tick.
    pub fn tick(
        &mut self,
        mut input: InputFrame,
        now: Duration,
        budget: OutputBudget,
        mut send: impl FnMut(&[u8]) -> bool,
    ) -> Result<WorkReport, DriverError> {
        let mut output = Output::new(budget, &mut send);
        let result = self.advance_time(now).and_then(|()| {
            self.expire();
            self.drain_received(&mut output.report);
            if self.session.is_none() {
                return self.hello(&mut output);
            }
            input = input.sanitized();
            input.sequence = self.next_sequence;
            self.next_sequence = self.next_sequence.wrapping_add(1);
            if self.pending.len() == MAX_PENDING_INPUTS {
                self.pending.pop_front();
                output.report.inputs_evicted += 1;
            }
            let shot_tick = (input.buttons & crate::BUTTON_FIRE != 0)
                .then(|| self.rendered_tick_unchecked(self.now));
            self.pending.push_back(TimedInput { input, shot_tick });
            self.input_dirty = true;
            if let Some(local) = &mut self.predicted {
                predict(self.round.as_ref(), &self.arena, local, input);
                output.report.prediction_steps += 1;
            }
            output.report.tick_executed = true;
            self.flush_acknowledgements(&mut output)?;
            self.flush_input(&mut output)
        });
        self.finish(output.report, result)?;
        Ok(self.last_report)
    }
    fn receive_packet(
        &mut self,
        packet: ServerPacket,
        bytes: &[u8],
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
        match packet {
            ServerPacket::Welcome {
                nonce,
                session,
                player_id,
                ..
            } => {
                if self.reconnect_required
                    || nonce != self.nonce
                    || (self.session_nonce == Some(nonce)
                        && (self.session != Some(session) || self.id != Some(player_id)))
                {
                    output.report.packets_rejected += 1;
                } else if self.session != Some(session) || self.session_nonce != Some(nonce) {
                    // A nonce identifies one handshake/session. Supporting a new
                    // nonce here also preserves legacy private harness setup.
                    self.clear_session();
                    self.session_nonce = Some(nonce);
                    self.session = Some(session);
                    self.id = Some(player_id);
                    self.last_snapshot = Some(self.now);
                    self.predicted = Some(Player::new(player_id, self.arena.spawn(player_id)));
                }
            }
            ServerPacket::Snapshot(snapshot) => self.accept_snapshot(snapshot, bytes, output)?,
            ServerPacket::Extension(packet) => self.extension(packet, output)?,
            ServerPacket::ReliableEvent { session, event } => {
                if Some(session) != self.session || SessionEvent::decode(&event.payload).is_err() {
                    output.report.packets_rejected += 1;
                    return Ok(());
                }
                let duplicate = event.sequence < self.event_receiver.next_sequence()
                    || self.event_receiver.contains_buffered(event.sequence);
                let capacity =
                    (MAX_DELIVERED_EVENTS - self.delivered_events.len()).min(reliable::EVENT_LIMIT);
                let ack_retained = self.event_acks.contains(&event.sequence);
                // Reserve a slot for the missing head when the combined cap is
                // smaller than the full sequence window. Capacity is absolute.
                let reserve_head = !duplicate
                    && event.sequence != self.event_receiver.next_sequence()
                    && !self
                        .event_receiver
                        .contains_buffered(self.event_receiver.next_sequence())
                    && self.event_receiver.buffered() + 1 >= capacity;
                if reserve_head || (!ack_retained && self.event_acks.len() == reliable::EVENT_LIMIT)
                {
                    output.report.events_refused += 1;
                    return Ok(());
                }
                match self.event_receiver.admit(&event, capacity) {
                    Ok(receipt) => {
                        match receipt {
                            Receipt::Retained => output.report.events_retained += 1,
                            Receipt::Duplicate => output.report.events_duplicate += 1,
                        }
                        if !ack_retained {
                            self.event_acks.push_back(event.sequence);
                        }
                    }
                    Err(reliable::Error::Full) => output.report.events_refused += 1,
                    Err(_) => {
                        output.report.events_refused += 1;
                        output.report.packets_rejected += 1;
                    }
                }
            }
        }
        Ok(())
    }
    fn extension(&mut self, packet: SX, output: &mut Output<'_>) -> Result<(), DriverError> {
        let session = match &packet {
            SX::SnapshotDelta { session, .. }
            | SX::Pong { session, .. }
            | SX::CommandAck { session, .. } => *session,
        };
        if Some(session) != self.session {
            output.report.packets_rejected += 1;
            return Ok(());
        }
        match packet {
            SX::SnapshotDelta { delta, .. } => {
                if self
                    .latest_tick
                    .is_some_and(|t| !sequence_newer(delta.tick, t))
                {
                    return Ok(());
                }
                let mut bytes = [0; MAX_PACKET_BYTES];
                let n = match self.snapshots.apply_into(&delta, &mut bytes) {
                    Ok(n) => n,
                    Err(_) => {
                        output.report.packets_rejected += 1;
                        self.full_requested = true;
                        return Ok(());
                    }
                };
                let Ok(ServerPacket::Snapshot(snapshot)) = decode_server(&bytes[..n]) else {
                    output.report.packets_rejected += 1;
                    self.full_requested = true;
                    return Ok(());
                };
                if snapshot.tick != delta.tick || snapshot.session != session {
                    output.report.packets_rejected += 1;
                    self.full_requested = true;
                    return Ok(());
                }
                let installed = output.report.snapshots_installed;
                self.accept_snapshot(snapshot, &bytes[..n], output)?;
                if output.report.snapshots_installed == installed {
                    self.full_requested = true;
                }
            }
            SX::Pong {
                nonce,
                client_us,
                receive_us,
                send_us,
                tick,
                ..
            } => {
                if !self.clock.observe(
                    nonce,
                    client_us,
                    receive_us,
                    send_us,
                    tick,
                    self.now.as_micros() as u64,
                ) {
                    output.report.packets_rejected += 1;
                }
            }
            SX::CommandAck { sequence, .. } => {
                self.commands.acknowledge(sequence);
            }
        }
        Ok(())
    }
    fn accept_snapshot(
        &mut self,
        snapshot: Snapshot,
        bytes: &[u8],
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
        if Some(snapshot.session) != self.session {
            output.report.packets_rejected += 1;
            return Ok(());
        }
        if self.id.is_none()
            || self
                .latest_tick
                .is_some_and(|last| !sequence_newer(snapshot.tick, last))
        {
            return Ok(());
        }
        let Some(state) = snapshot.players.iter().find(|p| Some(p.id) == self.id) else {
            output.report.packets_rejected += 1;
            return Ok(());
        };
        // All wire/epoch/roster checks have run through the actual decoder.
        // Preflight tick extension before installing or publishing anything.
        let mut unwrapper = self.unwrapper;
        let Some(extended) = unwrapper.observe(snapshot.tick) else {
            output.report.packets_rejected += 1;
            return Ok(());
        };
        self.snapshots.install(snapshot.tick, bytes)?;
        self.unwrapper = unwrapper;
        self.previous_tick = self.latest_tick.unwrap_or(snapshot.tick);
        self.latest_tick = Some(snapshot.tick);
        self.last_snapshot = Some(self.now);
        self.previous_epochs = self.epochs;
        self.epochs = snapshot.epochs;
        self.previous_players = std::mem::take(&mut self.players);
        self.players = snapshot.players.iter().map(Player::from_snapshot).collect();
        self.history.push(
            extended,
            RenderFrame {
                players: self.players.clone(),
                epochs: self.epochs,
            },
        );
        if snapshot.ack_valid {
            self.ack = Some(snapshot.ack_sequence);
            let before = self.pending.len();
            self.pending
                .retain(|v| sequence_newer(v.input.sequence, snapshot.ack_sequence));
            output.report.inputs_acknowledged += before - self.pending.len();
        }
        let mut corrected = Player::from_snapshot(state);
        for input in &self.pending {
            predict(
                self.round.as_ref(),
                &self.arena,
                &mut corrected,
                input.input,
            );
            output.report.replayed_inputs += 1;
        }
        self.predicted = Some(corrected);
        self.snapshot_ack = Some(snapshot.tick); // Coalesce only installed ACKs.
        self.full_requested = false;
        output.report.snapshots_installed += 1;
        Ok(())
    }
    fn drain_received(&mut self, report: &mut WorkReport) {
        // Decode and reserve final gameplay space before consuming an ordered
        // receipt. Round/lobby/results are applied once in this same order.
        let delivery = self.event_receiver.drain_ready(
            EVENT_WORK,
            EVENT_WORK * reliable::EVENT_BYTES,
            |_, bytes| {
                let event = SessionEvent::decode(bytes).expect("validated before retention");
                match event {
                    SessionEvent::Gameplay(e) => {
                        if self.delivered_events.len() == MAX_DELIVERED_EVENTS {
                            return false;
                        }
                        self.delivered_events.push_back(e);
                    }
                    SessionEvent::Lobby {
                        players, required, ..
                    } => {
                        self.lobby = Some((players, required));
                        self.round = None;
                    }
                    SessionEvent::Round(v) => {
                        self.lobby = None;
                        self.round = Some(v);
                        if let Some(state) = self.players.iter().find(|p| Some(p.id) == self.id) {
                            let mut corrected = state.clone();
                            for input in &self.pending {
                                predict(
                                    self.round.as_ref(),
                                    &self.arena,
                                    &mut corrected,
                                    input.input,
                                );
                                report.replayed_inputs += 1;
                            }
                            self.predicted = Some(corrected);
                        }
                    }
                    SessionEvent::ActionResult {
                        sequence, result, ..
                    } => {
                        self.action_result = Some((sequence, result));
                    }
                }
                true
            },
        );
        report.events_consumed += delivery.delivered;
        report.events_ready = delivery.remaining_ready;
    }
    fn flush_acknowledgements(&mut self, output: &mut Output<'_>) -> Result<(), DriverError> {
        let Some(session) = self.session else {
            return Ok(());
        };
        for _ in 0..reliable::EVENT_LIMIT {
            let Some(&sequence) = self.event_acks.front() else {
                break;
            };
            if !output.packet(&ClientPacket::EventAck { session, sequence })? {
                return Ok(());
            }
            self.event_acks.pop_front();
        }
        if let Some(tick) = self.snapshot_ack {
            if !output.packet(&ClientPacket::Extension(CX::SnapshotAck {
                session,
                tick: Some(tick),
            }))? {
                return Ok(());
            }
            self.snapshot_ack = None;
        }
        if self.full_requested
            && self
                .last_full_request
                .is_none_or(|t| self.now - t >= Duration::from_millis(100))
            && output.packet(&ClientPacket::Extension(CX::SnapshotAck {
                session,
                tick: None,
            }))?
        {
            self.full_requested = false;
            self.last_full_request = Some(self.now);
        }
        Ok(())
    }
    fn hello(&mut self, output: &mut Output<'_>) -> Result<(), DriverError> {
        if self.session.is_none()
            && !self.reconnect_required
            && self
                .last_hello
                .is_none_or(|t| self.now - t >= HELLO_INTERVAL)
            && output.packet(&ClientPacket::Hello { nonce: self.nonce })?
        {
            self.last_hello = Some(self.now);
        }
        Ok(())
    }
    fn flush_input(&mut self, output: &mut Output<'_>) -> Result<(), DriverError> {
        let Some(session) = self.session else {
            return Ok(());
        };
        if !self.input_dirty {
            return Ok(());
        }
        let count = self.pending.len().min(MAX_INPUTS_PER_PACKET);
        if count == 0 {
            self.input_dirty = false;
            return Ok(());
        }
        let inputs = self
            .pending
            .iter()
            .skip(self.pending.len() - count)
            .copied()
            .collect();
        if output.packet(&ClientPacket::Extension(CX::Inputs { session, inputs }))? {
            self.input_dirty = false;
        }
        Ok(())
    }
    fn flush_commands(&mut self, output: &mut Output<'_>) -> Result<(), DriverError> {
        let Some(session) = self.session else {
            return Ok(());
        };
        let mut error = None;
        let report = self.commands.send_due(
            self.now.as_millis() as u64,
            EVENT_WORK,
            EVENT_WORK * reliable::EVENT_BYTES,
            |event| {
                let mut bytes = [0; MAX_PACKET_BYTES];
                match (BorrowedEvent::Command { session, event }).encode_into(&mut bytes) {
                    Ok(n) => output.offer(&bytes[..n]),
                    Err(e) => {
                        error = Some(e);
                        false
                    }
                }
            },
        );
        output.report.commands_remaining_due = report.remaining_due;
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(())
    }
    fn probe_clock(&mut self, output: &mut Output<'_>) -> Result<(), DriverError> {
        let Some(session) = self.session else {
            return Ok(());
        };
        if self
            .last_ping
            .is_some_and(|t| self.now - t < Duration::from_millis(500))
        {
            return Ok(());
        }
        // A refused probe must not consume/overwrite correlated probe storage.
        let mut clock = self.clock.clone();
        let Some((nonce, client_us)) = clock.probe(self.now.as_micros() as u64) else {
            return Ok(());
        };
        if output.packet(&ClientPacket::Extension(CX::Ping {
            session,
            nonce,
            client_us,
        }))? {
            self.clock = clock;
            self.last_ping = Some(self.now);
        }
        Ok(())
    }
    fn render_target(&self, now: Duration) -> Option<f64> {
        let latest = self.unwrapper.project(self.latest_tick?)? as f64;
        let previous = self
            .unwrapper
            .project(self.previous_tick)
            .unwrap_or(latest as u64) as f64;
        let fallback =
            previous + (latest - previous) * f64::from(self.interpolation_alpha_unchecked(now));
        let Some(t) = self
            .clock
            .render_tick(now.as_micros() as u64, super::super::TICK_RATE as u16)
        else {
            return Some(fallback);
        };
        let wrapped = (t.floor() as i128).rem_euclid(1i128 << 32) as u32;
        let Some(extended) = self.unwrapper.project(wrapped) else {
            return Some(previous);
        };
        Some(
            (extended as f64 + t.fract())
                .min(latest)
                .max(self.history.first_tick().unwrap_or(previous as u64) as f64),
        )
    }
    /// The authority tick actually used to present remote poses. Fire commands
    /// stamp this view; the server independently rejects claims outside 250 ms.
    fn rendered_tick_unchecked(&self, now: Duration) -> u32 {
        self.render_target(now)
            .map_or(self.server_tick(), |v| v.floor() as u64 as u32)
    }
    fn rendered_players_unchecked(&self, now: Duration) -> Vec<Player> {
        let Some(t) = self.render_target(now) else {
            return self.players.clone();
        };
        let Some((left, right, alpha)) = self.history.bracket(t) else {
            return self.interpolated_players(self.interpolation_alpha_unchecked(now));
        };
        self.players
            .iter()
            .map(|current| {
                let mut p = current.clone();
                let id = p.id as usize;
                if p.health == 0 {
                    return p;
                }
                if left.epochs[id] == self.epochs[id]
                    && right.epochs[id] == self.epochs[id]
                    && let (Some(old), Some(new)) = (
                        left.players.iter().find(|v| v.id == p.id),
                        right.players.iter().find(|v| v.id == p.id),
                    )
                    && old.health > 0
                    && new.health > 0
                    && old.position.distance_squared(new.position) < 25.
                    && current.position.distance_squared(new.position) < 25.
                {
                    p.position = old.position.lerp(new.position, alpha);
                    let delta = (new.yaw - old.yaw + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    p.yaw = old.yaw + delta * alpha;
                    p.pitch = old.pitch + (new.pitch - old.pitch) * alpha;
                    return p;
                }
                // Hold the first pose of a new life until the delayed view catches
                // up; never glide through the old life or snap back after respawn.
                if let Some(first) = self.history.iter().find_map(|(_, f)| {
                    if f.epochs[id] == self.epochs[id] {
                        f.players.iter().find(|v| v.id == p.id)
                    } else {
                        None
                    }
                }) && first.health > 0
                    && current.position.distance_squared(first.position) < 25.
                {
                    p.position = first.position;
                    p.yaw = first.yaw;
                    p.pitch = first.pitch;
                }
                p
            })
            .collect()
    }
    pub fn round_trip_seconds(&self) -> Option<f64> {
        self.clock.rtt_seconds()
    }
    pub fn jitter_seconds(&self) -> f64 {
        self.clock.jitter_seconds()
    }
    /// Interpolate the last two authority frames, intentionally one snapshot late.
    /// Supply alpha in `[0,1]`, or call `interpolation_alpha` for a time-based value.
    pub fn interpolated_players(&self, alpha: f32) -> Vec<Player> {
        let alpha = if alpha.is_finite() {
            alpha.clamp(0., 1.)
        } else {
            1.
        };
        self.players
            .iter()
            .map(|current| {
                let mut p = current.clone();
                if let Some(old) = self.previous_players.iter().find(|old| old.id == p.id) {
                    // Death/respawn and teleports should snap, not glide across the map.
                    if self.epochs[p.id as usize] == self.previous_epochs[p.id as usize]
                        && old.health > 0
                        && p.health > 0
                        && old.position.distance_squared(p.position) < 25.
                    {
                        p.position = old.position.lerp(p.position, alpha);
                        let delta = (p.yaw - old.yaw + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI;
                        p.yaw = old.yaw + delta * alpha;
                        p.pitch = old.pitch + (p.pitch - old.pitch) * alpha;
                    }
                }
                p
            })
            .collect()
    }
    fn interpolation_alpha_unchecked(&self, now: Duration) -> f32 {
        let interval = self
            .latest_tick
            .unwrap_or(0)
            .wrapping_sub(self.previous_tick)
            .clamp(1, 64) as f32
            * crate::FIXED_DT;
        (self
            .last_snapshot
            .map_or(interval, |time| (now - time).as_secs_f32())
            / interval)
            .clamp(0., 1.)
    }
    /// Buffered presentation sampled at supplied time. Preserves the legacy
    /// f64 absolute-tick precision limit (integer exactness through 2^53), life
    /// epoch/teleport discontinuities, and latest-roster behavior. Wire epochs
    /// remain low32; rotate sessions before tick/epoch reuse or serial ambiguity.
    pub fn rendered_players(&self, now: Duration) -> Result<Vec<Player>, DriverError> {
        self.validate_time(now)?;
        Ok(self.rendered_players_unchecked(now))
    }
    pub fn rendered_tick(&self, now: Duration) -> Result<u32, DriverError> {
        self.validate_time(now)?;
        Ok(self.rendered_tick_unchecked(now))
    }
    pub fn interpolation_alpha(&self, now: Duration) -> Result<f32, DriverError> {
        self.validate_time(now)?;
        Ok(self.interpolation_alpha_unchecked(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_math::Vec3;
    use ferravex_net::{decode_client, encode_server};
    fn receive(state: &mut ClientState, packet: ServerPacket) -> WorkReport {
        state
            .receive(
                &encode_server(&packet).unwrap(),
                Duration::ZERO,
                OutputBudget {
                    datagrams: 0,
                    bytes: 0,
                },
                |_| panic!("zero output"),
            )
            .unwrap()
    }
    fn connected(tick: u32) -> ClientState {
        let mut state = ClientState::new(Arena::default(), 10).unwrap();
        receive(
            &mut state,
            ServerPacket::Welcome {
                nonce: 10,
                session: 20,
                player_id: 0,
                tick,
            },
        );
        let mut game = crate::Match::default();
        game.spawn_player(0);
        game.spawn_player(1);
        receive(
            &mut state,
            ServerPacket::Snapshot(Snapshot {
                session: 20,
                tick,
                epochs: game.life_epochs(),
                players: game.players.iter().map(Player::snapshot).collect(),
                ..Default::default()
            }),
        );
        // Remove the ACK retained by the deliberately zero-budget fixture.
        state
            .poll(Duration::ZERO, Default::default(), |_| true)
            .unwrap();
        state
    }
    fn event(sequence: u64) -> ServerPacket {
        ServerPacket::ReliableEvent {
            session: 20,
            event: reliable::EventPacket {
                sequence,
                payload: SessionEvent::Gameplay(NetworkEvent {
                    tick: 1,
                    event: crate::GameEvent::Respawned { player: 0 },
                })
                .encode()
                .unwrap(),
            },
        }
    }
    #[test]
    fn input_sequence_wrap_reconciliation_and_life_discontinuity_use_existing_predictor() {
        // Boundary initialization is an explicit codec fixture, not fabricated
        // gameplay outcomes. All prediction after it uses production tick calls.
        let mut state = connected(u32::MAX - 1);
        state.next_sequence = u32::MAX - 1;
        let original = state.snapshots.get(u32::MAX - 1).unwrap().to_vec();
        let mut sent = Vec::new();
        for _ in 0..3 {
            state
                .tick(
                    InputFrame {
                        move_z: 1.,
                        ..Default::default()
                    },
                    Duration::ZERO,
                    Default::default(),
                    |bytes| {
                        sent.push(decode_client(bytes).unwrap());
                        true
                    },
                )
                .unwrap();
        }
        let ClientPacket::Extension(CX::Inputs { inputs, .. }) = sent.last().unwrap() else {
            panic!("input envelope");
        };
        assert_eq!(
            inputs.iter().map(|i| i.input.sequence).collect::<Vec<_>>(),
            [u32::MAX - 1, u32::MAX, 0]
        );
        let ServerPacket::Snapshot(mut snapshot) = decode_server(&original).unwrap() else {
            panic!();
        };
        snapshot.tick = 0;
        snapshot.ack_valid = true;
        snapshot.ack_sequence = u32::MAX;
        snapshot.epochs[1] = snapshot.epochs[1].wrapping_add(1);
        snapshot
            .players
            .iter_mut()
            .find(|p| p.id == 1)
            .unwrap()
            .position = [1., 0., 1.];
        let report = receive(&mut state, ServerPacket::Snapshot(snapshot));
        assert_eq!(report.inputs_acknowledged, 2);
        assert_eq!(report.replayed_inputs, 1);
        assert_eq!(state.server_tick(), 0);
        assert_eq!(state.pending.front().unwrap().input.sequence, 0);
        let rendered = state.rendered_players(Duration::ZERO).unwrap();
        assert_eq!(
            rendered.iter().find(|p| p.id == 1).unwrap().position,
            Vec3::new(1., 0., 1.)
        );
        assert_eq!(
            state.interpolated_players(0.)[1].position,
            Vec3::new(1., 0., 1.)
        );
        assert_eq!(state.snapshots.len(), 2);
        assert!(state.unwrapper.project(0).unwrap() > u64::from(u32::MAX));
    }
    #[test]
    fn newest_input_tail_eviction_is_explicit_and_poll_retries_it_without_predicting() {
        let mut state = connected(0);
        for i in 0..(MAX_PENDING_INPUTS + 3) {
            let report = state
                .tick(
                    InputFrame::default(),
                    Duration::ZERO,
                    OutputBudget {
                        datagrams: 0,
                        bytes: 0,
                    },
                    |_| panic!("zero output"),
                )
                .unwrap();
            assert_eq!(report.inputs_evicted, usize::from(i >= MAX_PENDING_INPUTS));
            assert_eq!(report.prediction_steps, 1);
        }
        assert_eq!(state.pending.front().unwrap().input.sequence, 3);
        assert_eq!(state.pending.back().unwrap().input.sequence, 130);
        let player = state.local_player().unwrap().clone();
        let mut tail = Vec::new();
        let report = state
            .poll(Duration::ZERO, Default::default(), |b| {
                if let ClientPacket::Extension(CX::Inputs { inputs, .. }) =
                    decode_client(b).unwrap()
                {
                    tail = inputs;
                }
                true
            })
            .unwrap();
        assert!(!report.tick_executed);
        assert_eq!(report.prediction_steps, 0);
        assert_eq!(tail.len(), MAX_INPUTS_PER_PACKET);
        assert_eq!(tail.first().unwrap().input.sequence, 125);
        assert_eq!(tail.last().unwrap().input.sequence, 130);
        assert_eq!(state.local_player(), Some(&player));
    }
    #[test]
    fn tight_combined_capacity_reserves_missing_head_and_does_not_consume_on_refusal() {
        let mut state = connected(0);
        let e = NetworkEvent {
            tick: 1,
            event: crate::GameEvent::Respawned { player: 0 },
        };
        state.delivered_events.resize(MAX_DELIVERED_EVENTS - 1, e);
        let report = receive(&mut state, event(1));
        assert_eq!(report.events_refused, 1);
        assert_eq!(report.acknowledgements_pending, 0);
        assert_eq!(state.event_receiver.next_sequence(), 0);
        let report = receive(&mut state, event(0));
        assert_eq!(report.events_consumed, 1);
        assert_eq!(report.events_retained, 1);
        assert_eq!(report.events_delivered, MAX_DELIVERED_EVENTS);
        let report = receive(&mut state, event(1));
        assert_eq!(report.events_refused, 1);
        assert_eq!(state.event_receiver.next_sequence(), 1);
        state.drain_events();
        let report = receive(&mut state, event(1));
        assert_eq!(report.events_retained, 1);
        assert_eq!(state.drain_events().len(), 1);
        assert_eq!(state.event_receiver.next_sequence(), 2);
    }
    #[test]
    fn exact_envelope_budget_and_refused_command_leave_retry_head_due() {
        let mut state = connected(0);
        // Direct command bytes set up sender capacity, not application actions.
        state
            .commands
            .queue(&encode_action(Action::BuyArmor))
            .unwrap();
        state
            .commands
            .queue(&encode_action(Action::BuyArmor))
            .unwrap();
        let mut callbacks = 0;
        let short = state
            .poll(
                Duration::ZERO,
                OutputBudget {
                    datagrams: 8,
                    bytes: 25,
                },
                |_| {
                    callbacks += 1;
                    true
                },
            )
            .unwrap();
        assert_eq!(callbacks, 0); // Current command envelope is 24 + two payload bytes.
        assert_eq!(short.commands_remaining_due, 2);
        let exact = state
            .poll(
                Duration::ZERO,
                OutputBudget {
                    datagrams: 8,
                    bytes: 26,
                },
                |_| {
                    callbacks += 1;
                    true
                },
            )
            .unwrap();
        assert_eq!(exact.bytes_accepted, 26);
        assert_eq!(exact.commands_remaining_due, 1);
        let refused = state
            .poll(Duration::ZERO, Default::default(), |_| false)
            .unwrap();
        assert_eq!(refused.commands_remaining_due, 1);
        let retry = state
            .poll(Duration::ZERO, Default::default(), |_| true)
            .unwrap();
        assert_eq!(retry.commands_remaining_due, 0);
        assert_eq!(state.commands.pending(), 2); // Only a session ACK removes them.
    }
    #[test]
    fn invalid_reconstructed_tick_missing_member_or_orphan_epoch_never_install() {
        let mut state = connected(0);
        let base = state.snapshots.get(0).unwrap().to_vec();
        let ServerPacket::Snapshot(mut snapshot) = decode_server(&base).unwrap() else {
            panic!();
        };
        snapshot.tick = 1;
        let next = encode_server(&ServerPacket::Snapshot(snapshot.clone())).unwrap();
        let delta = ferravex_net::delta::Delta::between(0, &base, 2, &next).unwrap();
        let report = receive(
            &mut state,
            ServerPacket::Extension(SX::SnapshotDelta { session: 20, delta }),
        );
        assert_eq!(report.packets_rejected, 1);
        assert_eq!(state.server_tick(), 0);
        assert_eq!(state.retained_baseline_count(), 1);
        snapshot.players.retain(|p| p.id != 0);
        snapshot.epochs[0] = 0;
        let report = receive(&mut state, ServerPacket::Snapshot(snapshot.clone()));
        assert_eq!(report.packets_rejected, 1);
        assert_eq!(report.snapshots_installed, 0);
        // Encode valid bytes first, then corrupt an absent epoch slot. The
        // shared decoder must reject the complete reconstruction before install.
        let mut wire = encode_server(&ServerPacket::Snapshot(snapshot)).unwrap();
        // Header is six magic/version/kind bytes, session8, tick4, ack4, bool1.
        let valid = decode_server(&wire).unwrap();
        assert!(matches!(valid, ServerPacket::Snapshot(_)));
        // Corrupt the first byte of absent slot fifteen in the epoch block.
        wire[23 + 15 * 4] = 1;
        let report = state
            .receive(&wire, Duration::ZERO, Default::default(), |_| true)
            .unwrap();
        assert_eq!(report.packets_rejected, 1);
        assert_eq!(state.retained_baseline_count(), 1);
        assert_eq!(state.server_tick(), 0);
    }
    #[test]
    fn failed_exact_baseline_install_never_publishes_or_acks_and_reconnect_recovers() {
        let mut state = connected(0);
        let ServerPacket::Snapshot(mut snapshot) =
            decode_server(state.snapshots.get(0).unwrap()).unwrap()
        else {
            panic!();
        };
        snapshot.tick = (1 << 31) - 1;
        receive(&mut state, ServerPacket::Snapshot(snapshot.clone()));
        state
            .poll(Duration::ZERO, Default::default(), |_| true)
            .unwrap();
        let old = state.local_player().unwrap().clone();
        // Latest is serial-newer, but retaining it with the oldest zero would
        // cross the exact store's half-space. This needs an outer epoch reset.
        snapshot.tick = 1 << 31;
        let bytes = encode_server(&ServerPacket::Snapshot(snapshot)).unwrap();
        let result = state.receive(&bytes, Duration::ZERO, Default::default(), |_| {
            panic!("uninstalled ACK")
        });
        assert!(matches!(
            result,
            Err(DriverError::Baseline(BaselineError::TickWindow { .. }))
        ));
        assert_eq!(state.last_report().errors, 1);
        assert_eq!(state.last_report().snapshots_installed, 0);
        assert_eq!(state.last_report().acknowledgements_pending, 0);
        assert_eq!(state.server_tick(), (1 << 31) - 1);
        assert_eq!(state.local_player(), Some(&old));
        assert_eq!(state.snapshots.len(), 2);
        // Poll does not spin/retry the failed installation. The host can perform
        // the required fresh-nonce reconnect without exposing that snapshot.
        assert!(
            !state
                .poll(Duration::ZERO, Default::default(), |_| true)
                .unwrap()
                .tick_executed
        );
        state.reconnect(11, Duration::ZERO).unwrap();
        assert_eq!(state.snapshots.len(), 0);
        assert_eq!(state.server_tick(), 0);
    }
}
