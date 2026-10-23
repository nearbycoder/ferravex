use super::server_state::{DriverError, Output, Peer, ServerState};
use super::*;
use ferravex_net::baselines::{AckOutcome, Selection};
use ferravex_net::{ClientExtension as CX, ServerExtension as SX, delta::Delta};
impl ServerState {
    pub(super) fn extension(
        &mut self,
        addr: SocketAddr,
        packet: CX,
        now: Duration,
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
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
            output.report.packets_rejected += 1;
            return Ok(());
        };
        if let CX::Command { event, .. } = packet {
            return self.receive_command(index, event, now, output);
        }
        let peer = &mut self.peers[index];
        match packet {
            CX::Inputs { inputs, .. } => {
                peer.last_seen = now;
                for input in inputs {
                    Self::admit_input(peer, input, output);
                }
            }
            CX::SnapshotAck { tick, .. } => {
                if let Some(tick) = tick {
                    if peer.snapshots.acknowledge(tick) == AckOutcome::Unknown {
                        output.report.packets_rejected += 1;
                    }
                } else {
                    peer.snapshots.reset();
                }
            }
            CX::Ping {
                nonce, client_us, ..
            } => {
                if peer
                    .last_ping
                    .is_some_and(|p| now - p < Duration::from_millis(100))
                {
                    return Ok(());
                }
                // A refused Pong can be retried at the same time. No reflection
                // is accepted until full local datagram admission succeeds.
                if output.packet(
                    addr,
                    &ServerPacket::Extension(SX::Pong {
                        session,
                        nonce,
                        client_us,
                        receive_us: now.as_micros() as u64,
                        send_us: now.as_micros() as u64,
                        tick: self.game.tick,
                    }),
                )? {
                    peer.last_ping = Some(now);
                }
                peer.last_seen = now;
            }
            CX::Command { .. } => unreachable!("handled above"),
        }
        Ok(())
    }
    pub(super) fn send_snapshot(
        game: &Match,
        peer: &mut Peer,
        output: &mut Output<'_>,
    ) -> Result<(), DriverError> {
        let snapshot = Snapshot {
            epochs: game.life_epochs(),
            session: peer.session,
            tick: game.tick,
            ack_sequence: peer.ack.unwrap_or(0),
            ack_valid: peer.ack.is_some(),
            players: game.players.iter().map(Player::snapshot).collect(),
        };
        let mut full = [0; MAX_PACKET_BYTES];
        let n = ferravex_net::encode_server_into(&ServerPacket::Snapshot(snapshot), &mut full)?;
        let full = &full[..n];
        let accepted =
            match peer
                .snapshots
                .select_into(game.tick, full, &mut peer.delta, 24, MAX_PACKET_BYTES)
            {
                Selection::Delta { .. } => {
                    // Reuse the one checked shared codec and recover Delta capacity.
                    let delta = std::mem::replace(
                        &mut peer.delta,
                        Delta {
                            base_tick: 0,
                            tick: 0,
                            bytes: Vec::new(),
                        },
                    );
                    let packet = ServerPacket::Extension(SX::SnapshotDelta {
                        session: peer.session,
                        delta,
                    });
                    let mut wire = [0; MAX_PACKET_BYTES];
                    let encoded = ferravex_net::encode_server_into(&packet, &mut wire);
                    if let ServerPacket::Extension(SX::SnapshotDelta { delta, .. }) = packet {
                        peer.delta = delta;
                    }
                    match encoded {
                        Ok(n) => output.offer(peer.addr, &wire[..n]),
                        Err(_) => output.offer(peer.addr, full), // bounded full fallback
                    }
                }
                Selection::Full(_) => output.offer(peer.addr, full),
            };
        if accepted {
            peer.snapshots.record_sent(game.tick, full)?;
        }
        Ok(())
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
        assert!(server.peers[0].snapshots.baseline().is_some());
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
        client.state.snapshots.reset();
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
        assert!(server.peers[0].snapshots.baseline().is_none());
        assert!(server.peers[0].snapshots.is_empty());
        for _ in 0..3 {
            server.tick().unwrap();
        }
        client.poll().unwrap();
        assert_eq!(client.server_tick(), 9, "next full frame recovers");
        assert_eq!(client.local_player(), server.game.player(0));
        server.poll().unwrap();
        let before = server.peers[0].snapshots.baseline().unwrap().0;
        let rejected = server.rejected_packets;
        let bad = encode_client(&ClientPacket::Extension(CX::SnapshotAck {
            session: server.peers[0].session,
            tick: Some(123456),
        }))
        .unwrap();
        client.socket.send(&bad).unwrap();
        server.poll().unwrap();
        assert_eq!(server.peers[0].snapshots.baseline().unwrap().0, before);
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
            client.receive_datagram(&bytes).unwrap();
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
