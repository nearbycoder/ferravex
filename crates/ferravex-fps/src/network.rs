use super::{Arena, InputFrame, MAX_PLAYERS, Match, NetworkEvent, Player, step_player};
use ferravex_net::reliable::{EventReceiver, EventSender};
#[cfg(not(test))]
use ferravex_net::udp::Socket;
use ferravex_net::{
    ClientPacket, InputQueue, MAX_INPUTS_PER_PACKET, MAX_PACKET_BYTES, ServerPacket, Snapshot,
    decode_client, decode_server, encode_client, encode_server, sequence_newer,
};
use std::{
    collections::{VecDeque, hash_map::RandomState},
    hash::{BuildHasher, Hasher},
    io,
    net::{SocketAddr, ToSocketAddrs},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const POLL_BUDGET: usize = 128;
const MAX_PENDING_INPUTS: usize = 128;
const PEER_TIMEOUT: Duration = Duration::from_secs(5);
const HELLO_INTERVAL: Duration = Duration::from_millis(250);
const SNAPSHOT_INTERVAL: u32 = 3;
const EVENT_SEND_BUDGET: usize = 8;
const MAX_DELIVERED_EVENTS: usize = 512;

// Loopback delivery is asynchronous on macOS and Windows. The test harness
// gives real UDP packets a bounded delivery window, including deliberately
// dropped/reordered packets. Runtime sockets remain strictly nonblocking.
#[cfg(test)]
struct Socket(ferravex_net::udp::Socket);
#[cfg(test)]
impl std::ops::Deref for Socket {
    type Target = ferravex_net::udp::Socket;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(test)]
impl Socket {
    fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        ferravex_net::udp::Socket::bind(addr).map(Self)
    }
    fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        ferravex_net::udp::Socket::connect(addr).map(Self)
    }
    fn recv_from(
        &self,
        buffer: &mut [u8; MAX_PACKET_BYTES + 1],
    ) -> io::Result<Option<(usize, SocketAddr)>> {
        let deadline = Instant::now() + Duration::from_millis(20);
        loop {
            match self.0.recv_from(buffer)? {
                Some(packet) => return Ok(Some(packet)),
                None if Instant::now() >= deadline => return Ok(None),
                None => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }
}

// RandomState obtains per-instance random keys from the standard library. This
// session cookie rejects accidental/spoofed input, but is not an auth protocol.
fn nonce() -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    h.finish().max(1)
}
fn send_ok(result: io::Result<usize>) -> io::Result<()> {
    match result {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e),
    }
}
fn protocol_error(error: ferravex_net::ProtocolError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

struct Peer {
    addr: SocketAddr,
    nonce: u64,
    session: u64,
    id: u8,
    inputs: InputQueue,
    last_input: InputFrame,
    last_input_tick: u32,
    ack: Option<u32>,
    last_seen: Instant,
    last_hello: Instant,
    events: EventSender,
}

/// A native nonblocking authoritative server. Call `tick` at exactly 64 Hz.
/// Each client can advance at most one simulation input per server tick.
/// Network traffic per poll and input queues are bounded independently.
pub struct GameServer {
    socket: Socket,
    peers: Vec<Peer>,
    pub game: Match,
    pub rejected_packets: u64,
    pub slow_event_disconnects: u64,
    started: Instant,
}
impl GameServer {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::with_arena(addr, Arena::default())
    }
    pub fn with_arena(addr: impl ToSocketAddrs, arena: Arena) -> io::Result<Self> {
        Ok(Self {
            socket: Socket::bind(addr)?,
            peers: Vec::with_capacity(MAX_PLAYERS),
            game: Match::new(arena),
            rejected_packets: 0,
            slow_event_disconnects: 0,
            started: Instant::now(),
        })
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    pub fn pending_event_count(&self) -> usize {
        self.peers.iter().map(|peer| peer.events.pending()).sum()
    }
    pub fn poll(&mut self) -> io::Result<()> {
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for _ in 0..POLL_BUDGET {
            let received = match self.socket.recv_from(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    self.rejected_packets += 1;
                    continue;
                }
                other => other?,
            };
            let Some((n, addr)) = received else {
                break;
            };
            let Ok(packet) = decode_client(&buffer[..n]) else {
                self.rejected_packets += 1;
                continue;
            };
            match packet {
                ClientPacket::EventAck { session, sequence } => {
                    let Some(peer) = self
                        .peers
                        .iter_mut()
                        .find(|peer| peer.addr == addr && peer.session == session)
                    else {
                        self.rejected_packets += 1;
                        continue;
                    };
                    if peer.events.acknowledge(sequence) {
                        peer.last_seen = Instant::now();
                    }
                }
                ClientPacket::Hello {
                    nonce: client_nonce,
                } => {
                    let now = Instant::now();
                    if let Some(peer) = self.peers.iter_mut().find(|p| p.addr == addr) {
                        // Bound handshake reflection and session churn independently
                        // of the global receive budget. Lost welcomes retry at 250ms.
                        if now.duration_since(peer.last_hello) < HELLO_INTERVAL {
                            continue;
                        }
                        peer.last_hello = now;
                        // A fresh client on the same endpoint starts a session but
                        // cannot use Hello to restore health, ammunition, or position.
                        if peer.nonce != client_nonce {
                            peer.nonce = client_nonce;
                            peer.session = nonce();
                            peer.inputs = InputQueue::default();
                            peer.events = EventSender::default();
                            peer.last_input = InputFrame::default();
                            peer.ack = None;
                            peer.last_input_tick = self.game.tick;
                            if let Some(player) = self.game.player(peer.id) {
                                peer.last_input.yaw = player.yaw;
                                peer.last_input.pitch = player.pitch;
                            }
                        }
                        peer.last_seen = Instant::now();
                    } else {
                        if self.peers.len() >= MAX_PLAYERS {
                            self.rejected_packets += 1;
                            continue;
                        }
                        let Some(id) =
                            (0..MAX_PLAYERS as u8).find(|id| self.game.player(*id).is_none())
                        else {
                            continue;
                        };
                        self.game.spawn_player(id);
                        self.peers.push(Peer {
                            addr,
                            nonce: client_nonce,
                            session: nonce(),
                            id,
                            inputs: InputQueue::default(),
                            last_input: InputFrame::default(),
                            last_input_tick: self.game.tick,
                            ack: None,
                            last_seen: now,
                            last_hello: now,
                            events: EventSender::default(),
                        });
                    }
                    let peer = self
                        .peers
                        .iter()
                        .find(|p| p.addr == addr)
                        .expect("peer inserted");
                    let welcome = encode_server(&ServerPacket::Welcome {
                        nonce: peer.nonce,
                        session: peer.session,
                        player_id: peer.id,
                        tick: self.game.tick,
                    })
                    .map_err(protocol_error)?;
                    send_ok(self.socket.send_to(&welcome, addr))?;
                    self.send_snapshot(peer)?;
                }
                ClientPacket::Input { session, inputs } => {
                    let Some(peer) = self
                        .peers
                        .iter_mut()
                        .find(|p| p.addr == addr && p.session == session)
                    else {
                        self.rejected_packets += 1;
                        continue;
                    };
                    peer.last_seen = Instant::now();
                    for input in inputs {
                        peer.inputs.push(input);
                    }
                }
            }
        }
        Ok(())
    }
    fn send_snapshot(&self, peer: &Peer) -> io::Result<()> {
        let snapshot = Snapshot {
            session: peer.session,
            tick: self.game.tick,
            ack_sequence: peer.ack.unwrap_or(0),
            ack_valid: peer.ack.is_some(),
            players: self.game.players.iter().map(Player::snapshot).collect(),
        };
        let bytes = encode_server(&ServerPacket::Snapshot(snapshot)).map_err(protocol_error)?;
        send_ok(self.socket.send_to(&bytes, peer.addr))
    }
    pub fn tick(&mut self) -> io::Result<Vec<super::GameEvent>> {
        self.poll()?;
        let now = Instant::now();
        self.peers.retain(|p| {
            if now.duration_since(p.last_seen) > PEER_TIMEOUT {
                self.game.remove_player(p.id);
                false
            } else {
                true
            }
        });
        let mut commands = [(0, InputFrame::default()); MAX_PLAYERS];
        let mut command_count = 0;
        for peer in &mut self.peers {
            if let Some(input) = peer.inputs.pop_latest() {
                peer.last_input = input;
                peer.last_input_tick = self.game.tick;
                peer.ack = Some(input.sequence);
            }
            let mut input = peer.last_input;
            // Do not leave movement/fire held forever when packets stop arriving.
            if self.game.tick.wrapping_sub(peer.last_input_tick) > 12 {
                input.move_x = 0.;
                input.move_z = 0.;
                input.buttons = 0;
            }
            commands[command_count] = (peer.id, input);
            command_count += 1;
        }
        let events = self.game.step(&commands[..command_count]);
        let payloads: Vec<_> = events
            .iter()
            .map(|event| {
                NetworkEvent {
                    tick: self.game.tick,
                    event: *event,
                }
                .encode()
            })
            .collect::<Result<_, _>>()
            .map_err(protocol_error)?;
        let mut slow = [false; MAX_PLAYERS];
        for peer in &mut self.peers {
            for payload in &payloads {
                if peer.events.queue(payload).is_err() {
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
        if self.game.tick.is_multiple_of(SNAPSHOT_INTERVAL) {
            for peer in &self.peers {
                self.send_snapshot(peer)?;
            }
        }
        self.flush_events()?;
        Ok(events)
    }
    fn flush_events(&mut self) -> io::Result<()> {
        let now = self.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        for peer in &mut self.peers {
            for event in peer.events.due(now, EVENT_SEND_BUDGET) {
                let bytes = encode_server(&ServerPacket::ReliableEvent {
                    session: peer.session,
                    event,
                })
                .map_err(protocol_error)?;
                send_ok(self.socket.send_to(&bytes, peer.addr))?;
            }
        }
        Ok(())
    }
}

/// Native prediction/reconciliation client. `tick` runs one local fixed step;
/// `poll` can additionally run each render frame. Up to six input commands are
/// repeated per datagram to tolerate isolated packet loss without reliable UDP.
/// Remote actors expose both authoritative and buffered interpolated samples.
pub struct GameClient {
    socket: Socket,
    arena: Arena,
    nonce: u64,
    session: Option<u64>,
    id: Option<u8>,
    next_sequence: u32,
    pending: VecDeque<InputFrame>,
    predicted: Option<Player>,
    players: Vec<Player>,
    previous_players: Vec<Player>,
    latest_tick: Option<u32>,
    previous_tick: u32,
    last_snapshot: Option<Instant>,
    last_hello: Option<Instant>,
    ack: Option<u32>,
    pub rejected_packets: u64,
    event_receiver: EventReceiver,
    delivered_events: VecDeque<NetworkEvent>,
}
impl GameClient {
    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::with_arena(addr, Arena::default())
    }
    pub fn with_arena(addr: impl ToSocketAddrs, arena: Arena) -> io::Result<Self> {
        let mut this = Self {
            socket: Socket::connect(addr)?,
            arena,
            nonce: nonce(),
            session: None,
            id: None,
            next_sequence: 0,
            pending: VecDeque::with_capacity(MAX_PENDING_INPUTS),
            predicted: None,
            players: vec![],
            previous_players: vec![],
            latest_tick: None,
            previous_tick: 0,
            last_snapshot: None,
            last_hello: None,
            ack: None,
            rejected_packets: 0,
            event_receiver: EventReceiver::default(),
            delivered_events: VecDeque::new(),
        };
        this.hello()?;
        Ok(this)
    }
    fn hello(&mut self) -> io::Result<()> {
        if self
            .last_hello
            .is_none_or(|sent| sent.elapsed() >= HELLO_INTERVAL)
        {
            let bytes = encode_client(&ClientPacket::Hello { nonce: self.nonce })
                .map_err(protocol_error)?;
            send_ok(self.socket.send(&bytes))?;
            self.last_hello = Some(Instant::now());
        }
        Ok(())
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
    /// Deliver reliable authority events exactly once in server sequence order.
    /// Drain regularly. An application that never drains eventually applies
    /// backpressure and is disconnected instead of consuming unbounded memory.
    pub fn drain_events(&mut self) -> Vec<NetworkEvent> {
        self.delivered_events.drain(..).collect()
    }
    pub fn pending_event_count(&self) -> usize {
        self.delivered_events.len()
    }
    pub fn pending_input_count(&self) -> usize {
        self.pending.len()
    }
    pub fn acknowledged_sequence(&self) -> Option<u32> {
        self.ack
    }
    pub fn snapshot_age(&self) -> Option<Duration> {
        self.last_snapshot.map(|t| t.elapsed())
    }
    pub fn tick(&mut self, mut input: InputFrame) -> io::Result<()> {
        self.poll()?;
        let Some(session) = self.session else {
            return self.hello();
        };
        input = input.sanitized();
        input.sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.pending.len() == MAX_PENDING_INPUTS {
            self.pending.pop_front();
        }
        self.pending.push_back(input);
        if let Some(local) = &mut self.predicted {
            step_player(local, input, &self.arena);
        }
        let count = self.pending.len().min(MAX_INPUTS_PER_PACKET);
        let inputs = self
            .pending
            .iter()
            .skip(self.pending.len() - count)
            .copied()
            .collect();
        let bytes =
            encode_client(&ClientPacket::Input { session, inputs }).map_err(protocol_error)?;
        send_ok(self.socket.send(&bytes))
    }
    pub fn poll(&mut self) -> io::Result<()> {
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for _ in 0..POLL_BUDGET {
            let received = match self.socket.recv_from(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    self.rejected_packets += 1;
                    continue;
                }
                other => other?,
            };
            let Some((n, _)) = received else {
                break;
            };
            let Ok(packet) = decode_server(&buffer[..n]) else {
                self.rejected_packets += 1;
                continue;
            };
            match packet {
                ServerPacket::ReliableEvent { session, event } => {
                    if Some(session) != self.session
                        || NetworkEvent::decode(&event.payload).is_err()
                    {
                        self.rejected_packets += 1;
                        continue;
                    }
                    let new = event.sequence >= self.event_receiver.next_sequence()
                        && !self.event_receiver.contains_buffered(event.sequence);
                    if new
                        && self.delivered_events.len() + self.event_receiver.buffered()
                            >= MAX_DELIVERED_EVENTS
                    {
                        // Do not ACK data for which the application has no room.
                        continue;
                    }
                    let delivered = match self.event_receiver.receive(&event) {
                        Ok(delivered) => delivered,
                        Err(_) => {
                            self.rejected_packets += 1;
                            continue;
                        }
                    };
                    for packet in delivered {
                        self.delivered_events.push_back(
                            NetworkEvent::decode(&packet.payload)
                                .expect("payload validated before buffering"),
                        );
                    }
                    let ack = encode_client(&ClientPacket::EventAck {
                        session,
                        sequence: event.sequence,
                    })
                    .map_err(protocol_error)?;
                    send_ok(self.socket.send(&ack))?;
                }
                ServerPacket::Welcome {
                    nonce,
                    session,
                    player_id,
                    ..
                } => {
                    if nonce != self.nonce {
                        self.rejected_packets += 1;
                        continue;
                    }
                    if self.session != Some(session) {
                        self.session = Some(session);
                        self.id = Some(player_id);
                        self.pending.clear();
                        self.event_receiver = EventReceiver::default();
                        self.delivered_events.clear();
                        self.ack = None;
                        self.latest_tick = None;
                        self.last_snapshot = Some(Instant::now());
                        self.predicted = Some(Player::new(player_id, self.arena.spawn(player_id)));
                    }
                }
                ServerPacket::Snapshot(snapshot) => {
                    if Some(snapshot.session) != self.session {
                        self.rejected_packets += 1;
                        continue;
                    }
                    if self.id.is_none()
                        || self
                            .latest_tick
                            .is_some_and(|last| !sequence_newer(snapshot.tick, last))
                    {
                        continue;
                    }
                    let Some(state) = snapshot.players.iter().find(|p| Some(p.id) == self.id)
                    else {
                        self.rejected_packets += 1;
                        continue;
                    };
                    self.previous_tick = self.latest_tick.unwrap_or(snapshot.tick);
                    self.latest_tick = Some(snapshot.tick);
                    self.last_snapshot = Some(Instant::now());
                    self.previous_players = std::mem::take(&mut self.players);
                    self.players = snapshot.players.iter().map(Player::from_snapshot).collect();
                    if snapshot.ack_valid {
                        self.ack = Some(snapshot.ack_sequence);
                        self.pending
                            .retain(|input| sequence_newer(input.sequence, snapshot.ack_sequence));
                    }
                    let mut corrected = Player::from_snapshot(state);
                    for input in &self.pending {
                        step_player(&mut corrected, *input, &self.arena);
                    }
                    self.predicted = Some(corrected);
                }
            }
        }
        // Automatic reconnect after a timed-out server session; a new nonce makes
        // delayed Welcome packets from the old session harmless.
        if self
            .last_snapshot
            .is_some_and(|time| time.elapsed() > PEER_TIMEOUT)
        {
            self.session = None;
            self.id = None;
            self.predicted = None;
            self.pending.clear();
            self.event_receiver = EventReceiver::default();
            self.delivered_events.clear();
            self.nonce = nonce();
            self.latest_tick = None;
            self.last_snapshot = None;
            self.last_hello = None;
            self.hello()?;
        }
        Ok(())
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
                    if old.health > 0
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
    pub fn interpolation_alpha(&self) -> f32 {
        let interval = self
            .latest_tick
            .unwrap_or(0)
            .wrapping_sub(self.previous_tick)
            .clamp(1, 64) as f32
            * super::FIXED_DT;
        (self
            .last_snapshot
            .map_or(interval, |time| time.elapsed().as_secs_f32())
            / interval)
            .clamp(0., 1.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_handshake_prediction_ack_and_authority() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![ferravex_math::Vec3::ZERO],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        assert!(client.is_connected());
        for sequence in 0..90 {
            client
                .tick(InputFrame {
                    sequence,
                    move_z: 1.,
                    ..Default::default()
                })
                .unwrap();
            server.tick().unwrap();
            client.poll().unwrap();
        }
        assert_eq!(server.peer_count(), 1);
        assert_eq!(client.player_id(), Some(0));
        assert_eq!(client.acknowledged_sequence(), Some(89));
        assert_eq!(client.pending_input_count(), 0);
        assert!(
            (client.local_player().unwrap().position - server.game.player(0).unwrap().position)
                .length()
                < 0.0001
        );
        assert!(server.game.player(0).unwrap().position.z < -8.);
    }
    #[test]
    fn flooding_inputs_cannot_speed_up_simulation_and_bad_cookie_is_rejected() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![ferravex_math::Vec3::ZERO],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        for _ in 0..200 {
            client
                .tick(InputFrame {
                    move_z: 1.,
                    ..Default::default()
                })
                .unwrap();
        }
        server.tick().unwrap();
        assert_eq!(server.game.tick, 1);
        assert!(
            server.game.player(0).unwrap().position.length()
                < super::super::WALK_SPEED * super::super::FIXED_DT
        );
        assert!(server.peers[0].inputs.len() <= ferravex_net::MAX_QUEUED_INPUTS);
        let bad = encode_client(&ClientPacket::Input {
            session: client.session.unwrap().wrapping_add(1).max(1),
            inputs: vec![InputFrame::default()],
        })
        .unwrap();
        client.socket.send(&bad).unwrap();
        server.poll().unwrap();
        assert!(server.rejected_packets > 0);
    }
    #[test]
    fn loss_redundancy_reordering_and_stale_session_are_handled() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![ferravex_math::Vec3::ZERO],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for sequence in 0..30 {
            client
                .tick(InputFrame {
                    move_z: 1.,
                    ..Default::default()
                })
                .unwrap();
            // Drop two input datagrams. The next redundant packet recovers them.
            if sequence == 2 || sequence == 3 {
                assert!(server.socket.recv_from(&mut buffer).unwrap().is_some());
            }
            server.tick().unwrap();
            client.poll().unwrap();
        }
        assert_eq!(client.pending_input_count(), 0);
        for _ in 0..6 {
            server.tick().unwrap();
            client.poll().unwrap();
        }
        assert_eq!(client.acknowledged_sequence(), Some(29));
        assert_eq!(client.pending_input_count(), 0);
        assert_eq!(
            client.local_player().unwrap(),
            server.game.player(0).unwrap()
        );
        let current = client.server_tick();
        let position = client.local_player().unwrap().position;
        let peer = &server.peers[0];
        let mut snapshot = Snapshot {
            session: peer.session,
            tick: current.wrapping_sub(3),
            ack_valid: true,
            ack_sequence: 29,
            players: server.game.players.iter().map(Player::snapshot).collect(),
        };
        snapshot.players[0].position = [100., 0., 100.];
        let reordered = encode_server(&ServerPacket::Snapshot(snapshot.clone())).unwrap();
        server.socket.send_to(&reordered, peer.addr).unwrap();
        client.poll().unwrap();
        assert_eq!(client.server_tick(), current);
        assert_eq!(client.local_player().unwrap().position, position);
        // A numerically newer snapshot from an old session must not overwrite state.
        snapshot.tick = current.wrapping_add(10);
        snapshot.session = peer.session.wrapping_add(1).max(1);
        let stale = encode_server(&ServerPacket::Snapshot(snapshot)).unwrap();
        server.socket.send_to(&stale, peer.addr).unwrap();
        client.poll().unwrap();
        assert_eq!(client.server_tick(), current);
        assert_eq!(client.local_player().unwrap().position, position);
        assert_eq!(client.rejected_packets, 1);
    }
    #[test]
    fn two_clients_receive_distinct_identity_authoritative_damage_and_slot_reuse() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![
                ferravex_math::Vec3::ZERO,
                ferravex_math::Vec3::new(0., 0., -5.),
            ],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        let mut first =
            GameClient::with_arena(server.local_addr().unwrap(), arena.clone()).unwrap();
        let mut second =
            GameClient::with_arena(server.local_addr().unwrap(), arena.clone()).unwrap();
        server.poll().unwrap();
        first.poll().unwrap();
        second.poll().unwrap();
        assert_eq!(first.player_id(), Some(0));
        assert_eq!(second.player_id(), Some(1));
        assert_ne!(first.session, second.session);
        for tick in 0..3 {
            first
                .tick(InputFrame {
                    buttons: if tick == 0 {
                        super::super::BUTTON_FIRE
                    } else {
                        0
                    },
                    ..Default::default()
                })
                .unwrap();
            second.tick(InputFrame::default()).unwrap();
            server.tick().unwrap();
            first.poll().unwrap();
            second.poll().unwrap();
        }
        assert_eq!(server.game.player(1).unwrap().health, 0);
        assert_eq!(second.local_player().unwrap().health, 0);
        assert_eq!(first.players().len(), 2);
        assert_eq!(
            first.players().iter().find(|p| p.id == 1).unwrap().health,
            0
        );
        // Advance the peer deadline directly; the timeout path needs no wall-clock sleep.
        server
            .peers
            .iter_mut()
            .find(|p| p.id == 1)
            .unwrap()
            .last_seen = Instant::now() - PEER_TIMEOUT - Duration::from_secs(1);
        server.tick().unwrap();
        assert_eq!(server.peer_count(), 1);
        assert!(server.game.player(1).is_none());
        let mut replacement = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        server.poll().unwrap();
        replacement.poll().unwrap();
        assert_eq!(replacement.player_id(), Some(1));
        assert_eq!(replacement.local_player().unwrap().health, 100);
        assert_ne!(replacement.session, second.session);
    }

    #[test]
    fn reconnect_preserves_authority_state_and_hello_replies_are_throttled() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(server.local_addr().unwrap()).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        let player = server.game.player_mut(0).unwrap();
        player.health = 34;
        player.ammo = 7;
        player.position.x = 3.;
        let prior = player.clone();
        let old_session = server.peers[0].session;
        client.nonce = client.nonce.wrapping_add(1);
        let hello = encode_client(&ClientPacket::Hello {
            nonce: client.nonce,
        })
        .unwrap();
        client.socket.send(&hello).unwrap();
        server.poll().unwrap();
        assert_eq!(
            server.peers[0].session, old_session,
            "immediate session churn is throttled"
        );
        server.peers[0].last_hello = Instant::now() - HELLO_INTERVAL;
        client.socket.send(&hello).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        assert_ne!(server.peers[0].session, old_session);
        assert_eq!(server.game.player(0).unwrap(), &prior);
        assert_eq!(client.local_player().unwrap().health, 34);
        assert_eq!(client.local_player().unwrap().ammo, 7);
    }

    #[test]
    fn repeated_packet_loss_does_not_accumulate_control_latency() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![ferravex_math::Vec3::ZERO],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for tick in 0..300 {
            client
                .tick(InputFrame {
                    move_z: 1.,
                    ..Default::default()
                })
                .unwrap();
            if tick % 5 == 1 {
                assert!(server.socket.recv_from(&mut buffer).unwrap().is_some());
            }
            server.tick().unwrap();
            client.poll().unwrap();
            assert!(
                server.peers[0].inputs.is_empty(),
                "no accumulated stale controls"
            );
            assert!(
                client.pending_input_count() <= 3,
                "acks stay within snapshot interval despite sustained loss"
            );
        }
        assert_eq!(client.acknowledged_sequence(), Some(299));
        assert_eq!(
            client.local_player().unwrap(),
            server.game.player(0).unwrap()
        );
    }
    #[test]
    fn oversized_datagram_cannot_stop_processing_following_valid_packet() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        sender
            .send_to(&[0; MAX_PACKET_BYTES + 100], address)
            .unwrap();
        let hello = encode_client(&ClientPacket::Hello { nonce: 42 }).unwrap();
        sender.send_to(&hello, address).unwrap();
        server.poll().unwrap();
        assert_eq!(server.rejected_packets, 1);
        assert_eq!(server.peer_count(), 1);
    }
    #[test]
    fn client_discards_oversized_datagram_and_accepts_following_welcome() {
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(sender.local_addr().unwrap()).unwrap();
        let address = client.socket.local_addr().unwrap();
        sender
            .send_to(&[0; MAX_PACKET_BYTES + 100], address)
            .unwrap();
        let welcome = encode_server(&ServerPacket::Welcome {
            nonce: client.nonce,
            session: 42,
            player_id: 0,
            tick: 0,
        })
        .unwrap();
        sender.send_to(&welcome, address).unwrap();
        client.poll().unwrap();
        assert_eq!(client.rejected_packets, 1);
        assert!(client.is_connected());
    }
    fn event_payload(player: u8) -> Vec<u8> {
        NetworkEvent {
            tick: 44,
            event: super::super::GameEvent::Respawned { player },
        }
        .encode()
        .unwrap()
    }
    #[test]
    fn reliable_authority_events_survive_loss_reorder_and_duplicates_on_udp() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![
                ferravex_math::Vec3::ZERO,
                ferravex_math::Vec3::new(0., 0., -5.),
            ],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        server.game.spawn_player(1);
        client
            .tick(InputFrame {
                buttons: super::super::BUTTON_FIRE,
                ..InputFrame::default()
            })
            .unwrap();
        let expected = server.tick().unwrap();
        assert_eq!(expected.len(), 3);
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        let mut packets = Vec::new();
        while let Some((n, _)) = client.socket.recv_from(&mut buffer).unwrap() {
            if matches!(
                decode_server(&buffer[..n]),
                Ok(ServerPacket::ReliableEvent { .. })
            ) {
                packets.push(buffer[..n].to_vec());
            }
        }
        assert_eq!(packets.len(), 3);
        let addr = server.peers[0].addr;
        // Drop event zero; reorder and duplicate the later packets over real UDP.
        for i in [2, 2, 1] {
            server.socket.send_to(&packets[i], addr).unwrap();
        }
        client.poll().unwrap();
        assert!(client.drain_events().is_empty());
        server.poll().unwrap();
        assert_eq!(server.pending_event_count(), 1);
        // Advance the retry clock without sleeping in a unit test.
        server.started = Instant::now() - Duration::from_millis(250);
        server.flush_events().unwrap();
        client.poll().unwrap();
        let delivered = client.drain_events();
        assert_eq!(
            delivered
                .iter()
                .map(|event| event.event)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(delivered.iter().all(|event| event.tick == 1));
        server.poll().unwrap();
        assert_eq!(server.pending_event_count(), 0);
        server.socket.send_to(&packets[0], addr).unwrap();
        client.poll().unwrap();
        assert!(client.drain_events().is_empty());
    }
    #[test]
    fn reliable_event_sessions_reset_on_reconnect_and_reject_old_ack_and_packet() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(server.local_addr().unwrap()).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        let old_session = server.peers[0].session;
        let old_packet = encode_server(&ServerPacket::ReliableEvent {
            session: old_session,
            event: ferravex_net::reliable::EventPacket {
                sequence: 0,
                payload: event_payload(0),
            },
        })
        .unwrap();
        server.peers[0].events.queue(&event_payload(0)).unwrap();
        server.flush_events().unwrap();
        client.poll().unwrap();
        assert_eq!(client.pending_event_count(), 1);
        client.nonce = client.nonce.wrapping_add(1);
        server.peers[0].last_hello = Instant::now() - HELLO_INTERVAL;
        client
            .socket
            .send(
                &encode_client(&ClientPacket::Hello {
                    nonce: client.nonce,
                })
                .unwrap(),
            )
            .unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        assert!(client.drain_events().is_empty());
        assert_eq!(server.pending_event_count(), 0);
        assert_ne!(server.peers[0].session, old_session);
        server.peers[0].events.queue(&event_payload(1)).unwrap();
        client
            .socket
            .send(
                &encode_client(&ClientPacket::EventAck {
                    session: old_session,
                    sequence: 0,
                })
                .unwrap(),
            )
            .unwrap();
        server.poll().unwrap();
        assert_eq!(server.pending_event_count(), 1);
        let rejected = client.rejected_packets;
        server
            .socket
            .send_to(&old_packet, server.peers[0].addr)
            .unwrap();
        server.flush_events().unwrap();
        client.poll().unwrap();
        assert_eq!(client.rejected_packets, rejected + 1);
        let delivered = client.drain_events();
        assert_eq!(delivered.len(), 1);
        assert_eq!(
            delivered[0].event,
            super::super::GameEvent::Respawned { player: 1 }
        );
        server.poll().unwrap();
        assert_eq!(server.pending_event_count(), 0);
    }
    #[test]
    fn reliable_backpressure_never_acks_unstored_events_and_server_is_bounded() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(server.local_addr().unwrap()).unwrap();
        server.poll().unwrap();
        client.poll().unwrap();
        let event = NetworkEvent::decode(&event_payload(0)).unwrap();
        client.delivered_events.resize(MAX_DELIVERED_EVENTS, event);
        server.peers[0].events.queue(&event_payload(1)).unwrap();
        server.flush_events().unwrap();
        client.poll().unwrap();
        server.poll().unwrap();
        assert_eq!(client.pending_event_count(), MAX_DELIVERED_EVENTS);
        assert_eq!(server.pending_event_count(), 1);
        assert_eq!(client.event_receiver.next_sequence(), 0);
        client.drain_events();
        server.started = Instant::now() - Duration::from_millis(250);
        server.flush_events().unwrap();
        client.poll().unwrap();
        server.poll().unwrap();
        assert_eq!(client.drain_events().len(), 1);
        assert_eq!(server.pending_event_count(), 0);
        for _ in 0..ferravex_net::reliable::EVENT_LIMIT {
            server.peers[0].events.queue(&event_payload(0)).unwrap();
        }
        client
            .tick(InputFrame {
                buttons: super::super::BUTTON_FIRE,
                ..InputFrame::default()
            })
            .unwrap();
        server.tick().unwrap();
        assert_eq!(server.slow_event_disconnects, 1);
        assert_eq!(server.peer_count(), 0);
        assert!(server.game.player(0).is_none());
        assert_eq!(server.pending_event_count(), 0);
    }
}
