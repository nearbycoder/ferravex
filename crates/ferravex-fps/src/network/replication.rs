use super::*;
use ferravex_net::{
    ClientExtension as CX, ServerExtension as SX, clock::ClientClock, delta::Delta,
};
const BASELINES: usize = 32;
impl GameServer {
    pub(super) fn extension(&mut self, addr: SocketAddr, packet: CX) -> io::Result<()> {
        let session = match &packet {
            CX::Inputs { session, .. }
            | CX::SnapshotAck { session, .. }
            | CX::Ping { session, .. }
            | CX::Command { session, .. } => *session,
        };
        let Some(index) = self
            .peers
            .iter()
            .position(|p| p.addr == addr && p.session == session)
        else {
            self.rejected_packets += 1;
            return Ok(());
        };
        if let CX::Command { event, .. } = packet {
            return self.receive_command(index, event);
        }
        let peer = &mut self.peers[index];
        match packet {
            CX::Inputs { inputs, .. } => {
                peer.last_seen = Instant::now();
                for input in inputs {
                    peer.inputs.push_timed(input);
                }
            }
            CX::SnapshotAck { tick, .. } => {
                if let Some(tick) = tick {
                    if peer
                        .baseline
                        .as_ref()
                        .is_none_or(|(old, _)| sequence_newer(tick, *old))
                    {
                        if let Some((_, bytes)) = peer.snapshots.iter().find(|(t, _)| *t == tick) {
                            peer.baseline = Some((tick, bytes.clone()));
                        } else {
                            self.rejected_packets += 1;
                        }
                    }
                } else {
                    peer.baseline = None;
                    peer.snapshots.clear();
                }
            }
            CX::Ping {
                nonce, client_us, ..
            } => {
                if peer
                    .last_ping
                    .is_some_and(|p| p.elapsed() < Duration::from_millis(100))
                {
                    return Ok(());
                }
                let now = Instant::now();
                peer.last_ping = Some(now);
                peer.last_seen = now;
                let packet = ServerPacket::Extension(SX::Pong {
                    session,
                    nonce,
                    client_us,
                    receive_us: now.as_micros(),
                    send_us: Instant::now().as_micros(),
                    tick: self.game.tick,
                });
                let bytes = encode_server(&packet).map_err(protocol_error)?;
                send_ok(self.socket.send_to(&bytes, addr))?;
            }
            CX::Command { .. } => unreachable!("command handled above"),
        }
        Ok(())
    }
    pub(super) fn send_snapshot(socket: &Socket, game: &Match, peer: &mut Peer) -> io::Result<()> {
        let snapshot = Snapshot {
            epochs: game.life_epochs(),
            session: peer.session,
            tick: game.tick,
            ack_sequence: peer.ack.unwrap_or(0),
            ack_valid: peer.ack.is_some(),
            players: game.players.iter().map(Player::snapshot).collect(),
        };
        let full = encode_server(&ServerPacket::Snapshot(snapshot)).map_err(protocol_error)?;
        let mut packet = None;
        if let Some((tick, base)) = &peer.baseline
            && sequence_newer(game.tick, *tick)
            && base.len() == full.len()
            && let Ok(delta) = Delta::between(*tick, base, game.tick, &full)
            && delta.bytes.len() + 24 < full.len()
        {
            packet = Some(
                encode_server(&ServerPacket::Extension(SX::SnapshotDelta {
                    session: peer.session,
                    delta,
                }))
                .map_err(protocol_error)?,
            );
        }
        send_ok(socket.send_to(packet.as_deref().unwrap_or(&full), peer.addr))?;
        if peer.snapshots.back().is_none_or(|(t, _)| *t != game.tick) {
            if peer.snapshots.len() == BASELINES {
                peer.snapshots.pop_front();
            }
            peer.snapshots.push_back((game.tick, full));
        }
        Ok(())
    }
}
impl GameClient {
    pub(super) fn extension(&mut self, packet: SX) -> io::Result<()> {
        let session = match &packet {
            SX::SnapshotDelta { session, .. }
            | SX::Pong { session, .. }
            | SX::CommandAck { session, .. } => *session,
        };
        if Some(session) != self.session {
            self.rejected_packets += 1;
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
                let Some((_, base)) = self.snapshots.iter().find(|(t, _)| *t == delta.base_tick)
                else {
                    return self.request_full();
                };
                let Ok(bytes) = delta.apply(delta.base_tick, base) else {
                    self.rejected_packets += 1;
                    return self.request_full();
                };
                let Ok(ServerPacket::Snapshot(snapshot)) = decode_server(&bytes) else {
                    self.rejected_packets += 1;
                    return self.request_full();
                };
                if snapshot.tick != delta.tick || snapshot.session != session {
                    self.rejected_packets += 1;
                    return self.request_full();
                }
                self.accept_snapshot(snapshot, bytes)?;
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
                    Instant::now().as_micros(),
                ) {
                    self.rejected_packets += 1;
                }
            }
            SX::CommandAck { sequence, .. } => {
                self.commands.acknowledge(sequence);
            }
        }
        Ok(())
    }
    fn request_full(&mut self) -> io::Result<()> {
        if self
            .last_full_request
            .is_some_and(|t| t.elapsed() < Duration::from_millis(100))
        {
            return Ok(());
        }
        let Some(session) = self.session else {
            return Ok(());
        };
        self.last_full_request = Some(Instant::now());
        let bytes = encode_client(&ClientPacket::Extension(CX::SnapshotAck {
            session,
            tick: None,
        }))
        .map_err(protocol_error)?;
        send_ok(self.socket.send(&bytes))
    }
    pub(super) fn accept_snapshot(&mut self, snapshot: Snapshot, bytes: Vec<u8>) -> io::Result<()> {
        if Some(snapshot.session) != self.session {
            self.rejected_packets += 1;
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
            self.rejected_packets += 1;
            return Ok(());
        };
        self.previous_tick = self.latest_tick.unwrap_or(snapshot.tick);
        self.latest_tick = Some(snapshot.tick);
        self.last_snapshot = Some(Instant::now());
        self.previous_epochs = self.epochs;
        self.epochs = snapshot.epochs;
        self.previous_players = std::mem::take(&mut self.players);
        self.players = snapshot.players.iter().map(Player::from_snapshot).collect();
        if let Some(extended) = self.unwrapper.observe(snapshot.tick) {
            self.history.push(
                extended,
                RenderFrame {
                    players: self.players.clone(),
                    epochs: self.epochs,
                },
            );
        }
        if snapshot.ack_valid {
            self.ack = Some(snapshot.ack_sequence);
            self.pending
                .retain(|v| sequence_newer(v.input.sequence, snapshot.ack_sequence));
        }
        let mut corrected = Player::from_snapshot(state);
        for input in &self.pending {
            super::tactical::predict(
                self.round.as_ref(),
                &self.arena,
                &mut corrected,
                input.input,
            );
        }
        self.predicted = Some(corrected);
        if self.snapshots.len() == BASELINES {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back((snapshot.tick, bytes));
        let ack = encode_client(&ClientPacket::Extension(CX::SnapshotAck {
            session: snapshot.session,
            tick: Some(snapshot.tick),
        }))
        .map_err(protocol_error)?;
        send_ok(self.socket.send(&ack))
    }
    pub(super) fn probe_clock(&mut self) -> io::Result<()> {
        let Some(session) = self.session else {
            return Ok(());
        };
        if self
            .last_ping
            .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
        {
            return Ok(());
        }
        let now = Instant::now();
        let Some((nonce, client_us)) = self.clock.probe(now.as_micros()) else {
            return Ok(());
        };
        self.last_ping = Some(now);
        let bytes = encode_client(&ClientPacket::Extension(CX::Ping {
            session,
            nonce,
            client_us,
        }))
        .map_err(protocol_error)?;
        send_ok(self.socket.send(&bytes))
    }
    pub(super) fn clear_replication(&mut self) {
        self.commands = EventSender::default();
        self.round = None;
        self.lobby = None;
        self.action_result = None;
        self.clock = ClientClock::default();
        self.last_ping = None;
        self.last_full_request = None;
        self.snapshots.clear();
        self.unwrapper = ferravex_net::clock::TickUnwrapper::default();
        self.history = ferravex_net::history::History::new(32);
        self.previous_players.clear();
        self.players.clear();
        self.epochs = [0; MAX_PLAYERS];
        self.previous_epochs = [0; MAX_PLAYERS];
        self.previous_tick = 0;
    }
    fn render_target(&self) -> Option<f64> {
        let latest = self.unwrapper.project(self.latest_tick?)? as f64;
        let previous = self
            .unwrapper
            .project(self.previous_tick)
            .unwrap_or(latest as u64) as f64;
        let fallback = previous + (latest - previous) * f64::from(self.interpolation_alpha());
        let Some(t) = self
            .clock
            .render_tick(Instant::now().as_micros(), super::super::TICK_RATE as u16)
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
    pub fn rendered_tick(&self) -> u32 {
        self.render_target()
            .map_or(self.server_tick(), |v| v.floor() as u64 as u32)
    }
    pub fn rendered_players(&self) -> Vec<Player> {
        let Some(t) = self.render_target() else {
            return self.players.clone();
        };
        let Some((left, right, alpha)) = self.history.bracket(t) else {
            return self.interpolated_players(self.interpolation_alpha());
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
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_udp_clock_delta_lost_baseline_full_recovery_and_unknown_ack() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(server.local_addr().unwrap()).unwrap();
        connect_test_clients(&mut server, &mut [&mut client]);
        server.poll().unwrap();
        client.poll().unwrap();
        assert!(client.round_trip_seconds().is_some());
        assert!(server.peers[0].baseline.is_some());
        for _ in 0..3 {
            server.tick().unwrap();
        }
        let mut buffer = [0; MAX_PACKET_BYTES + 1];
        let mut packet = None;
        while let Some((n, _)) = client.socket.recv_from(&mut buffer).unwrap() {
            if matches!(
                decode_server(&buffer[..n]),
                Ok(ServerPacket::Extension(SX::SnapshotDelta { .. }))
            ) {
                packet = Some(buffer[..n].to_vec());
            }
        }
        let bytes = packet.expect("sparse authority should send a delta after exact ACK");
        server.socket.send_to(&bytes, server.peers[0].addr).unwrap();
        client.poll().unwrap();
        assert_eq!(client.server_tick(), 3);
        client.snapshots.clear();
        for _ in 0..3 {
            server.tick().unwrap();
        }
        client.poll().unwrap();
        assert_eq!(
            client.server_tick(),
            3,
            "missing baseline must not mutate state"
        );
        server.poll().unwrap();
        assert!(server.peers[0].baseline.is_none());
        assert!(server.peers[0].snapshots.is_empty());
        for _ in 0..3 {
            server.tick().unwrap();
        }
        client.poll().unwrap();
        assert_eq!(client.server_tick(), 9, "next full frame recovers");
        assert_eq!(client.local_player(), server.game.player(0));
        server.poll().unwrap();
        let before = server.peers[0].baseline.as_ref().unwrap().0;
        let rejected = server.rejected_packets;
        let bad = encode_client(&ClientPacket::Extension(CX::SnapshotAck {
            session: server.peers[0].session,
            tick: Some(123456),
        }))
        .unwrap();
        client.socket.send(&bad).unwrap();
        server.poll().unwrap();
        assert_eq!(server.peers[0].baseline.as_ref().unwrap().0, before);
        assert!(server.rejected_packets > rejected);
    }
    #[test]
    fn real_udp_timed_shot_rewinds_and_bad_claim_cannot_rearm_itself() {
        let arena = Arena {
            solids: vec![],
            spawns: vec![
                ferravex_math::Vec3::ZERO,
                ferravex_math::Vec3::new(0., 0., -6.),
            ],
        };
        let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
        server.game.spawn_player(1);
        let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
        connect_test_clients(&mut server, &mut [&mut client]);
        for _ in 0..15 {
            server
                .tick_with_local(&[(
                    1,
                    InputFrame {
                        move_x: 1.,
                        ..Default::default()
                    },
                )])
                .unwrap();
            client.poll().unwrap();
        }
        assert!(server.game.player(1).unwrap().position.x > 0.8);
        let send = |client: &GameClient, sequence, shot_tick| {
            let bytes = encode_client(&ClientPacket::Extension(CX::Inputs {
                session: client.session.unwrap(),
                inputs: vec![TimedInput {
                    input: InputFrame {
                        sequence,
                        buttons: super::super::super::BUTTON_FIRE,
                        move_z: 1.,
                        ..Default::default()
                    },
                    shot_tick: Some(shot_tick),
                }],
            }))
            .unwrap();
            client.socket.send(&bytes).unwrap();
        };
        send(&client, 100, 0);
        let events = server.tick().unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, super::super::super::GameEvent::Hit { victim: 1, .. }))
        );
        assert_eq!(server.game.player(1).unwrap().health, 0);
        server.game.player_mut(0).unwrap().fire_cooldown = 0;
        let ammo = server.game.player(0).unwrap().ammo;
        let position = server.game.player(0).unwrap().position;
        let future = server.game.tick + 1;
        send(&client, 101, future);
        let events = server.tick().unwrap();
        assert!(events.is_empty());
        assert_eq!(server.game.player(0).unwrap().ammo, ammo);
        assert!(server.game.player(0).unwrap().position != position);
        assert_eq!(server.rejected_shots, 1);
        server.tick().unwrap();
        assert_eq!(
            server.game.player(0).unwrap().ammo,
            ammo,
            "a previously invalid future claim must stay disarmed without a fresh input"
        );
    }
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod life_tests {
    use super::*;
    use ferravex_math::Vec3;
    #[test]
    fn interpolation_uses_latest_roster_and_never_blends_different_lives() {
        let mut server = GameServer::bind("127.0.0.1:0").unwrap();
        let mut client = GameClient::connect(server.local_addr().unwrap()).unwrap();
        connect_test_clients(&mut server, &mut [&mut client]);
        let session = client.session.unwrap();
        let mut apply = |tick, epoch, x: Option<f32>| {
            let mut players = vec![Player::new(0, Vec3::ZERO).snapshot()];
            let mut epochs = [0; MAX_PLAYERS];
            epochs[0] = 1;
            if let Some(x) = x {
                players.push(Player::new(1, Vec3::X * x).snapshot());
                epochs[1] = epoch;
            }
            let snapshot = Snapshot {
                session,
                tick,
                ack_sequence: 0,
                ack_valid: false,
                epochs,
                players,
            };
            let bytes = encode_server(&ServerPacket::Snapshot(snapshot.clone())).unwrap();
            client.accept_snapshot(snapshot, bytes).unwrap();
            client.rendered_players()
        };
        apply(3, 1, Some(2.));
        let current = apply(6, 2, Some(1.));
        assert_eq!(
            current.iter().find(|p| p.id == 1).unwrap().position.x,
            1.,
            "same-point respawn must not interpolate an old life"
        );
        let current = apply(9, 2, Some(1.5));
        assert!(
            (current.iter().find(|p| p.id == 1).unwrap().position.x - 1.).abs() < 0.01,
            "new life holds its first pose while delayed view catches up"
        );
        let current = apply(12, 0, None);
        assert!(
            !current.iter().any(|p| p.id == 1),
            "removed actors disappear from the current roster immediately"
        );
        let current = apply(15, 3, Some(1.));
        assert_eq!(current.iter().find(|p| p.id == 1).unwrap().position.x, 1.);
    }
}
