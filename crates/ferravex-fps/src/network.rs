#[derive(Clone)]
struct RenderFrame {
    players: Vec<Player>,
    epochs: [u32; MAX_PLAYERS],
}
pub mod client_state;
mod replication;
pub mod server_state;
mod tactical;
use super::{Arena, InputFrame, MAX_PLAYERS, Match, NetworkEvent, Player, step_player};
#[cfg(target_arch = "wasm32")]
use crate::browser::Socket;
use crate::tactical::View;
#[cfg(test)]
use ferravex_net::TimedInput;
#[cfg(test)]
use ferravex_net::encode_server;
use ferravex_net::time::Instant;
#[cfg(all(not(test), not(target_arch = "wasm32")))]
use ferravex_net::udp::Socket;
#[cfg(test)]
use ferravex_net::{ClientPacket, decode_server, encode_client};
use ferravex_net::{MAX_PACKET_BYTES, ServerPacket, Snapshot};
use ferravex_play::rounds::RuleError;
use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};
pub use tactical::TacticalOptions;

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
#[cfg(all(test, not(target_arch = "wasm32")))]
struct Socket(ferravex_net::udp::Socket);
#[cfg(all(test, not(target_arch = "wasm32")))]
impl std::ops::Deref for Socket {
    type Target = ferravex_net::udp::Socket;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(all(test, not(target_arch = "wasm32")))]
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
    #[cfg(target_arch = "wasm32")]
    return crate::browser::nonce();
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::{
            collections::hash_map::RandomState,
            hash::{BuildHasher, Hasher},
            time::{SystemTime, UNIX_EPOCH},
        };
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        h.finish().max(1)
    }
}
/// Native/Wasm nonblocking adapter around the transport-independent authority.
/// Call tick at exactly 64 Hz. Public game/counters/configuration remain available
/// through the owned ServerState; all protocol decisions live in that core.
pub struct GameServer {
    socket: Socket,
    state: server_state::ServerState,
    started: Instant,
}
impl std::ops::Deref for GameServer {
    type Target = server_state::ServerState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
impl std::ops::DerefMut for GameServer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}
fn driver_error(error: server_state::DriverError) -> io::Error {
    let kind = match &error {
        server_state::DriverError::InvalidLocalInputs
        | server_state::DriverError::TacticalConfiguration => io::ErrorKind::InvalidInput,
        server_state::DriverError::Reliable(_) => io::ErrorKind::Other,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}
// UDP and browser queue acceptance requires the complete datagram. WouldBlock,
// a short result and fatal transport errors must never advance retry timestamps.
fn offer_datagram(
    socket: &Socket,
    addr: SocketAddr,
    bytes: &[u8],
    error: &mut Option<io::Error>,
) -> bool {
    if error.is_some() {
        return false;
    }
    datagram_result(socket.send_to(bytes, addr), bytes.len(), error)
}
fn datagram_result(
    result: io::Result<usize>,
    expected: usize,
    error: &mut Option<io::Error>,
) -> bool {
    match result {
        Ok(n) if n == expected => true,
        Ok(_) => {
            *error = Some(io::Error::new(
                io::ErrorKind::WriteZero,
                "short authority datagram",
            ));
            false
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => false,
        Err(e) => {
            *error = Some(e);
            false
        }
    }
}
impl GameServer {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::with_arena(addr, Arena::default())
    }
    pub fn with_arena(addr: impl ToSocketAddrs, arena: Arena) -> io::Result<Self> {
        Ok(Self {
            socket: Socket::bind(addr)?,
            state: server_state::ServerState::new(arena),
            started: Instant::now(),
        })
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
    pub fn peer_count(&self) -> usize {
        self.state.peer_count()
    }
    pub fn pending_event_count(&self) -> usize {
        self.state.pending_event_count()
    }
    pub fn enable_tactical(&mut self, options: TacticalOptions) -> io::Result<()> {
        self.state
            .enable_tactical(options)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    }
    pub fn tactical_view(&self) -> Option<View> {
        self.state.tactical_view()
    }
    pub fn tactical_lobby(&self) -> Option<(usize, usize)> {
        self.state.tactical_lobby()
    }
    pub fn act_local(
        &mut self,
        id: u8,
        action: ferravex_play::rounds::Action,
    ) -> Result<(), RuleError> {
        self.state.act_local(id, action)
    }
    pub fn restart_tactical(&mut self) {
        self.state.restart_tactical();
    }
    pub fn poll(&mut self) -> io::Result<()> {
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for _ in 0..POLL_BUDGET {
            let received = match self.socket.recv_from(&mut buffer) {
                Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                    self.state.rejected_packets += 1;
                    continue;
                }
                other => other?,
            };
            let Some((n, addr)) = received else {
                break;
            };
            let mut error = None;
            let result = self.state.receive(
                addr,
                &buffer[..n],
                self.started.elapsed(),
                nonce,
                server_state::OutputBudget::default(),
                |to, bytes| offer_datagram(&self.socket, to, bytes, &mut error),
            );
            if let Some(error) = error {
                self.state.last_report.errors += 1;
                return Err(error);
            }
            result.map_err(driver_error)?;
        }
        self.flush_events()
    }
    pub fn tick(&mut self) -> io::Result<Vec<super::GameEvent>> {
        self.tick_with_local(&[])
    }
    pub fn tick_with_local(
        &mut self,
        local: &[(u8, InputFrame)],
    ) -> io::Result<Vec<super::GameEvent>> {
        // Preserve validation before receiving/stepping, including disconnected
        // local IDs and attempts to override a remote authority actor.
        self.state
            .validate_local(local)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        self.poll()?;
        let mut error = None;
        let result = self.state.tick(
            local,
            self.started.elapsed(),
            server_state::OutputBudget::default(),
            |to, bytes| offer_datagram(&self.socket, to, bytes, &mut error),
        );
        if let Some(error) = error {
            self.state.last_report.errors += 1;
            return Err(error);
        }
        result.map(|v| v.events).map_err(driver_error)
    }
    fn flush_events(&mut self) -> io::Result<()> {
        let mut error = None;
        let result = self.state.poll(
            self.started.elapsed(),
            server_state::OutputBudget::default(),
            |to, bytes| offer_datagram(&self.socket, to, bytes, &mut error),
        );
        if let Some(error) = error {
            self.state.last_report.errors += 1;
            return Err(error);
        }
        result.map(|_| ()).map_err(driver_error)
    }
}

/// Native/Wasm nonblocking adapter around one production ClientState. `tick`
/// executes one local fixed step; poll through local overlays on render frames.
/// Socket I/O, time sampling and fresh reconnect nonces remain at this boundary.
pub struct GameClient {
    socket: Socket,
    state: client_state::ClientState,
    started: Instant,
}
impl std::ops::Deref for GameClient {
    type Target = client_state::ClientState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
impl std::ops::DerefMut for GameClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}
fn client_driver_error(error: client_state::DriverError) -> io::Error {
    let kind = match error {
        client_state::DriverError::NotConnected => io::ErrorKind::NotConnected,
        client_state::DriverError::Reliable(_) => io::ErrorKind::WouldBlock,
        client_state::DriverError::InvalidNonce => io::ErrorKind::InvalidInput,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}
fn offer_client_datagram(socket: &Socket, bytes: &[u8], error: &mut Option<io::Error>) -> bool {
    if error.is_some() {
        return false;
    }
    datagram_result(socket.send(bytes), bytes.len(), error)
}
impl GameClient {
    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::with_arena(addr, Arena::default())
    }
    pub fn with_arena(addr: impl ToSocketAddrs, arena: Arena) -> io::Result<Self> {
        let mut this = Self {
            socket: Socket::connect(addr)?,
            state: client_state::ClientState::new(arena, nonce()).map_err(client_driver_error)?,
            started: Instant::now(),
        };
        this.hello()?;
        Ok(this)
    }
    fn hello(&mut self) -> io::Result<()> {
        self.maintain()
    }
    fn receive_datagram(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut error = None;
        let result =
            self.state
                .receive(bytes, self.started.elapsed(), Default::default(), |bytes| {
                    offer_client_datagram(&self.socket, bytes, &mut error)
                });
        if let Some(error) = error {
            self.state.last_report.errors += 1;
            return Err(error);
        }
        result.map(|_| ()).map_err(client_driver_error)
    }
    fn maintain(&mut self) -> io::Result<()> {
        // One initial poll plus one possible timeout reconnect. No repeated
        // transport/driver error loop; failures return to the caller immediately.
        for _ in 0..2 {
            if self.state.needs_reconnect() {
                let mut fresh = nonce();
                if fresh == self.state.nonce {
                    fresh = fresh.wrapping_add(1).max(1);
                }
                self.state
                    .reconnect(fresh, self.started.elapsed())
                    .map_err(client_driver_error)?;
            }
            let mut error = None;
            let result = self
                .state
                .poll(self.started.elapsed(), Default::default(), |bytes| {
                    offer_client_datagram(&self.socket, bytes, &mut error)
                });
            if let Some(error) = error {
                self.state.last_report.errors += 1;
                return Err(error);
            }
            result.map_err(client_driver_error)?;
            if !self.state.needs_reconnect() {
                break;
            }
        }
        Ok(())
    }
    pub fn poll(&mut self) -> io::Result<()> {
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        for _ in 0..POLL_BUDGET {
            let received = match self.socket.recv_from(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    self.state.rejected_packets = self.state.rejected_packets.saturating_add(1);
                    continue;
                }
                other => other?,
            };
            let Some((n, _)) = received else {
                break;
            };
            self.receive_datagram(&buffer[..n])?;
        }
        self.maintain()
    }
    pub fn tick(&mut self, input: InputFrame) -> io::Result<()> {
        self.poll()?;
        let mut error = None;
        let result = self
            .state
            .tick(input, self.started.elapsed(), Default::default(), |bytes| {
                offer_client_datagram(&self.socket, bytes, &mut error)
            });
        if let Some(error) = error {
            self.state.last_report.errors += 1;
            return Err(error);
        }
        result.map(|_| ()).map_err(client_driver_error)
    }
    pub fn player_id(&self) -> Option<u8> {
        self.state.player_id()
    }
    pub fn is_connected(&self) -> bool {
        self.state.is_connected()
    }
    pub fn local_player(&self) -> Option<&Player> {
        self.state.local_player()
    }
    pub fn players(&self) -> &[Player] {
        self.state.players()
    }
    pub fn server_tick(&self) -> u32 {
        self.state.server_tick()
    }
    pub fn drain_events(&mut self) -> Vec<NetworkEvent> {
        self.state.drain_events()
    }
    pub fn pending_event_count(&self) -> usize {
        self.state.pending_event_count()
    }
    pub fn pending_input_count(&self) -> usize {
        self.state.pending_input_count()
    }
    pub fn acknowledged_sequence(&self) -> Option<u32> {
        self.state.acknowledged_sequence()
    }
    pub fn snapshot_age(&self) -> Option<Duration> {
        self.state
            .snapshot_age(self.started.elapsed())
            .expect("adapter monotonic time")
    }
    pub fn interpolated_players(&self, alpha: f32) -> Vec<Player> {
        self.state.interpolated_players(alpha)
    }
    pub fn interpolation_alpha(&self) -> f32 {
        self.state
            .interpolation_alpha(self.started.elapsed())
            .expect("adapter monotonic time")
    }
    pub fn rendered_tick(&self) -> u32 {
        self.state
            .rendered_tick(self.started.elapsed())
            .expect("adapter monotonic time")
    }
    pub fn rendered_players(&self) -> Vec<Player> {
        self.state
            .rendered_players(self.started.elapsed())
            .expect("adapter monotonic time")
    }
    pub fn round_trip_seconds(&self) -> Option<f64> {
        self.state.round_trip_seconds()
    }
    pub fn jitter_seconds(&self) -> f64 {
        self.state.jitter_seconds()
    }
    pub fn tactical_lobby(&self) -> Option<(u8, u8)> {
        self.state.tactical_lobby()
    }
    pub fn tactical_view(&self) -> Option<&View> {
        self.state.tactical_view()
    }
    pub fn last_action_result(&self) -> Option<(u64, Result<(), RuleError>)> {
        self.state.last_action_result()
    }
    pub fn queue_action(&mut self, action: ferravex_play::rounds::Action) -> io::Result<u64> {
        self.state.queue_action(action).map_err(client_driver_error)
    }
    pub fn pending_command_count(&self) -> usize {
        self.state.pending_command_count()
    }
}

// Complete real transport setup before measuring gameplay/security behavior.
// Loopback packets may miss the small per-poll delivery window on a busy host.
#[cfg(all(test, not(target_arch = "wasm32")))]
fn connect_test_clients(server: &mut GameServer, clients: &mut [&mut GameClient]) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let tick = server.game.tick;
    while clients.iter().any(|client| !client.is_connected()) {
        for client in clients.iter_mut() {
            client.hello().unwrap();
        }
        server.poll().unwrap();
        for client in clients.iter_mut() {
            client.poll().unwrap();
        }
        assert!(
            clients.iter().all(|client| client.is_connected()) || Instant::now() < deadline,
            "loopback handshake did not complete"
        );
    }
    assert_eq!(
        server.game.tick, tick,
        "handshake must not advance simulation"
    );
}

#[cfg(all(test, not(target_arch = "wasm32")))]
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
            epochs: server.game.life_epochs(),
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
        connect_test_clients(&mut server, &mut [&mut first, &mut second]);
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
            .last_seen = Duration::ZERO;
        server.started = server.started - PEER_TIMEOUT - Duration::from_secs(1);
        let now = server.started.elapsed();
        for p in &mut server.state.peers {
            if p.id != 1 {
                p.last_seen = now;
            }
        }
        server.tick().unwrap();
        assert_eq!(server.peer_count(), 1);
        assert!(server.game.player(1).is_none());
        let mut replacement = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        connect_test_clients(&mut server, &mut [&mut replacement]);
        assert_eq!(replacement.player_id(), Some(1));
        assert_eq!(replacement.local_player().unwrap().health, 100);
        assert_ne!(replacement.session, second.session);
    }

    #[test]
    fn reconnect_preserves_authority_state_and_hello_replies_are_throttled() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(server.local_addr().unwrap()).unwrap();
        connect_test_clients(&mut server, &mut [&mut client]);
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
        server.started = server.started - HELLO_INTERVAL;
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
        server.started = server.started - Duration::from_millis(250);
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
        server.started = server.started - HELLO_INTERVAL;
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
        connect_test_clients(&mut server, &mut [&mut client]);
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
        server.started = server.started - Duration::from_millis(250);
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

#[cfg(test)]
mod authority_adapter_tests {
    use super::*;
    #[test]
    fn complete_datagram_admission_refuses_would_block_short_and_fatal_results() {
        let mut error = None;
        assert!(datagram_result(Ok(26), 26, &mut error));
        assert!(error.is_none());
        assert!(!datagram_result(
            Err(io::ErrorKind::WouldBlock.into()),
            26,
            &mut error
        ));
        assert!(error.is_none());
        assert!(!datagram_result(Ok(25), 26, &mut error));
        assert_eq!(error.take().unwrap().kind(), io::ErrorKind::WriteZero);
        assert!(!datagram_result(
            Err(io::ErrorKind::ConnectionReset.into()),
            26,
            &mut error
        ));
        assert_eq!(error.take().unwrap().kind(), io::ErrorKind::ConnectionReset);
    }
}
