//! Runnable deterministic composition of the two production drivers and actual
//! v5 codecs. Mailboxes have whole-datagram admission; loss is after admission,
//! refusal is before it. No sockets, sleeps, alternate predictor or fake outcomes.
use ferravex_fps::client_state::{ClientState, DriverError, OutputBudget, WorkReport};
use ferravex_fps::server_state::ServerState;
use ferravex_fps::tactical::SessionEvent;
use ferravex_fps::{Arena, BUTTON_FIRE, GameEvent, InputFrame, Player, TacticalOptions};
use ferravex_math::Vec3;
use ferravex_net::reliable::{EVENT_LIMIT, EventPacket};
use ferravex_net::{
    ClientExtension as CX, ClientPacket, MAX_PACKET_BYTES, ServerExtension as SX, ServerPacket,
    Snapshot, decode_client, decode_server, encode_server,
};
use std::{collections::VecDeque, net::SocketAddr, time::Duration};
const ZERO: OutputBudget = OutputBudget {
    datagrams: 0,
    bytes: 0,
};
const MAILBOX: usize = 16;
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn arena() -> Arena {
    Arena {
        solids: vec![],
        spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.)],
    }
}
fn admit(queue: &mut VecDeque<Vec<u8>>, bytes: &[u8]) -> bool {
    assert!(bytes.len() <= MAX_PACKET_BYTES);
    if queue.len() == MAILBOX {
        return false;
    }
    queue.push_back(bytes.to_vec());
    true
}
struct Rig {
    server: ServerState,
    client: ClientState,
    up: VecDeque<Vec<u8>>,
    down: VecDeque<Vec<u8>>,
    peer: SocketAddr,
    cookie: u64,
    now: Duration,
}
impl Rig {
    fn new() -> Self {
        Self {
            server: ServerState::new(arena()),
            client: ClientState::new(arena(), 10).unwrap(),
            up: VecDeque::new(),
            down: VecDeque::new(),
            peer: ([127, 0, 0, 1], 7001).into(),
            cookie: 100,
            now: Duration::ZERO,
        }
    }
    fn join(&mut self) {
        self.poll();
        self.pump();
        assert!(self.client.is_connected());
        self.poll();
        self.pump();
        assert!(self.client.round_trip_seconds().is_some());
    }
    fn poll(&mut self) -> WorkReport {
        self.client
            .poll(self.now, Default::default(), |b| admit(&mut self.up, b))
            .unwrap()
    }
    fn pump(&mut self) {
        // Bounded host work as well as bounded storage. Each pass delivers the
        // admitted prefix; assertion catches accidental protocol echo loops.
        for _ in 0..4 {
            for _ in 0..MAILBOX {
                let Some(bytes) = self.up.pop_front() else {
                    break;
                };
                self.server
                    .receive(
                        self.peer,
                        &bytes,
                        self.now,
                        || {
                            self.cookie += 1;
                            self.cookie
                        },
                        Default::default(),
                        |to, b| {
                            assert_eq!(to, self.peer);
                            admit(&mut self.down, b)
                        },
                    )
                    .unwrap();
            }
            for _ in 0..MAILBOX {
                let Some(bytes) = self.down.pop_front() else {
                    break;
                };
                self.client
                    .receive(&bytes, self.now, Default::default(), |b| {
                        admit(&mut self.up, b)
                    })
                    .unwrap();
            }
        }
        assert!(self.up.is_empty() && self.down.is_empty());
    }
    fn tick_client(&mut self, input: InputFrame) -> WorkReport {
        self.client
            .tick(input, self.now, Default::default(), |b| {
                admit(&mut self.up, b)
            })
            .unwrap()
    }
    fn tick_server(&mut self) {
        self.server
            .tick(&[], self.now, Default::default(), |to, b| {
                assert_eq!(to, self.peer);
                admit(&mut self.down, b)
            })
            .unwrap();
    }
    fn receive(&mut self, packet: ServerPacket, budget: OutputBudget) -> WorkReport {
        self.client
            .receive(&encode_server(&packet).unwrap(), self.now, budget, |b| {
                admit(&mut self.up, b)
            })
            .unwrap()
    }
    fn snapshot(&self, tick: u32) -> Snapshot {
        Snapshot {
            session: self.cookie,
            tick,
            epochs: self.server.game.life_epochs(),
            players: self
                .server
                .game
                .players
                .iter()
                .map(Player::snapshot)
                .collect(),
            ..Default::default()
        }
    }
}
fn event(session: u64, sequence: u64) -> ServerPacket {
    ServerPacket::ReliableEvent {
        session,
        event: EventPacket {
            sequence,
            payload: SessionEvent::Gameplay(ferravex_fps::NetworkEvent {
                tick: 7,
                event: GameEvent::Respawned {
                    player: (sequence % 16) as u8,
                },
            })
            .encode()
            .unwrap(),
        },
    }
}
#[test]
fn production_recipe_loss_reorder_refusal_reconnect_and_presentation() {
    let mut rig = Rig::new();
    rig.join();
    let connected = rig.client.local_player().unwrap().clone();
    for _ in 0..20 {
        rig.poll();
        rig.pump();
    }
    assert_eq!(rig.client.local_player(), Some(&connected));
    assert_eq!(rig.server.game.tick, 0);
    let mut delivered = Vec::new();
    for tick in 0..120 {
        rig.now += Duration::from_micros(15_625);
        let report = rig.tick_client(InputFrame {
            move_z: 1.,
            ..Default::default()
        });
        assert!(report.tick_executed);
        if matches!(tick, 2 | 3 | 50) {
            // Drop admitted input datagrams; next real input carries redundancy.
            rig.up.retain(|b| {
                !matches!(
                    decode_client(b),
                    Ok(ClientPacket::Extension(CX::Inputs { .. }))
                )
            });
        }
        rig.pump();
        rig.tick_server();
        if tick % 17 == 0 {
            rig.down.clear();
        } // Accepted network loss.
        rig.down.make_contiguous().reverse(); // Reorder actual encoded snapshots/events.
        rig.pump();
        delivered.extend(rig.client.drain_events());
        let rendered = rig.client.rendered_players(rig.now).unwrap();
        assert_eq!(rendered.len(), rig.server.game.players.len());
        assert!(rendered.iter().all(|p| p.position.x.is_finite()));
    }
    for _ in 0..6 {
        rig.now += ms(16);
        rig.tick_server();
        rig.pump();
    }
    assert_eq!(rig.client.pending_input_count(), 0);
    assert_eq!(rig.client.local_player(), rig.server.game.player(0));
    assert_eq!(rig.client.acknowledged_sequence(), Some(119));
    assert_eq!(rig.client.retained_baseline_count(), 32);
    assert!(rig.client.retained_baseline_bytes() <= 32 * MAX_PACKET_BYTES);
    let prior = rig.server.game.player(0).unwrap().clone();
    let old_session = rig.cookie;
    rig.client.reconnect(11, rig.now).unwrap();
    let refused = rig
        .client
        .poll(rig.now, Default::default(), |_| false)
        .unwrap();
    assert_eq!(refused.datagrams_refused, 1);
    rig.poll();
    rig.pump();
    assert!(rig.client.is_connected());
    assert_ne!(rig.cookie, old_session);
    assert_eq!(rig.server.game.player(0), Some(&prior));
    assert_eq!(rig.client.local_player(), Some(&prior));
    assert!(rig.client.drain_events().is_empty());
}
#[test]
fn pending_input_replay_matches_production_prediction_and_poll_never_ticks() {
    let mut rig = Rig::new();
    rig.join();
    let start = rig.client.local_player().unwrap().clone();
    for _ in 0..3 {
        rig.now += ms(16);
        rig.tick_client(InputFrame {
            move_z: 1.,
            ..Default::default()
        });
        rig.pump();
        rig.tick_server();
    }
    // The actual tick-three snapshot ACKs inputs 0..2. Predict two more
    // executed ticks before receiving it, then reconcile those two inputs.
    for _ in 0..2 {
        rig.now += ms(16);
        rig.tick_client(InputFrame {
            move_z: 1.,
            ..Default::default()
        });
    }
    let predicted = rig.client.local_player().unwrap().clone();
    assert_ne!(predicted.position, start.position);
    let bytes = rig.down.pop_front().expect("actual authority snapshot");
    assert!(matches!(
        decode_server(&bytes),
        Ok(ServerPacket::Extension(SX::SnapshotDelta { .. })) | Ok(ServerPacket::Snapshot(_))
    ));
    let replay = rig
        .client
        .receive(&bytes, rig.now, Default::default(), |b| {
            admit(&mut rig.up, b)
        })
        .unwrap();
    assert_eq!(replay.replayed_inputs, 2);
    assert_eq!(rig.client.pending_input_count(), 2);
    assert_eq!(rig.client.acknowledged_sequence(), Some(2));
    assert_eq!(rig.client.local_player(), Some(&predicted));
    let pending = rig.client.pending_input_count();
    let player = rig.client.local_player().unwrap().clone();
    rig.poll();
    rig.poll();
    assert_eq!(rig.client.pending_input_count(), pending);
    assert_eq!(rig.client.local_player(), Some(&player));
    assert_eq!(rig.server.game.tick, 3);
}
#[test]
fn zero_budget_and_refusal_retain_hello_input_snapshot_ack_and_probe() {
    let mut rig = Rig::new();
    let report = rig
        .client
        .poll(rig.now, ZERO, |_| panic!("zero budget called output"))
        .unwrap();
    assert_eq!(report.datagrams_accepted, 0);
    assert_eq!(report.datagrams_deferred, 1);
    assert!(
        rig.client
            .clock_diagnostics(rig.now)
            .unwrap()
            .pending_probes
            .iter()
            .all(Option::is_none)
    );
    rig.join();
    rig.now += ms(16);
    let report = rig
        .client
        .tick(
            InputFrame {
                move_z: 1.,
                ..Default::default()
            },
            rig.now,
            ZERO,
            |_| panic!("zero budget"),
        )
        .unwrap();
    assert!(report.tick_executed);
    assert_eq!(report.inputs_retained, 1);
    rig.poll();
    assert!(rig.up.iter().any(|b| matches!(
        decode_client(b),
        Ok(ClientPacket::Extension(CX::Inputs { .. }))
    )));
    rig.up.clear();
    let snapshot = rig.snapshot(1);
    let report = rig.receive(ServerPacket::Snapshot(snapshot), ZERO);
    assert_eq!(report.snapshots_installed, 1);
    assert_eq!(report.acknowledgements_pending, 1);
    rig.poll();
    assert!(rig.up.iter().any(|b| matches!(
        decode_client(b),
        Ok(ClientPacket::Extension(CX::SnapshotAck {
            tick: Some(1),
            ..
        }))
    )));
    rig.now = ms(600);
    rig.up.clear();
    let before = rig.client.clock_diagnostics(rig.now).unwrap();
    rig.client
        .poll(rig.now, Default::default(), |_| false)
        .unwrap();
    assert_eq!(rig.client.clock_diagnostics(rig.now).unwrap(), before);
    rig.poll();
    assert!(rig.up.iter().any(|b| matches!(
        decode_client(b),
        Ok(ClientPacket::Extension(CX::Ping { .. }))
    )));
}
#[test]
fn reliable_window_retains_missing_head_and_drains_without_more_packets() {
    let mut rig = Rig::new();
    rig.join();
    // Synthetic ingress exercises the exact full sequence window, not gameplay.
    for sequence in (1..EVENT_LIMIT as u64).rev() {
        let report = rig.receive(event(rig.cookie, sequence), Default::default());
        assert_eq!(report.events_retained, 1);
        assert_eq!(report.events_consumed, 0);
        rig.up.clear(); // Drop admitted ACKs.
    }
    assert_eq!(rig.client.retained_event_count(), EVENT_LIMIT - 1);
    let report = rig.receive(event(rig.cookie, 0), Default::default());
    assert_eq!(report.events_retained, 1);
    assert_eq!(report.events_consumed, 8);
    assert_eq!(report.events_ready, EVENT_LIMIT - 8);
    let mut all = rig.client.drain_events();
    for _ in 0..31 {
        rig.poll();
        all.extend(rig.client.drain_events());
        rig.up.clear();
    }
    assert_eq!(all.len(), EVENT_LIMIT);
    for (sequence, event) in all.iter().enumerate() {
        assert_eq!(
            event.event,
            GameEvent::Respawned {
                player: (sequence % 16) as u8
            }
        );
    }
    assert_eq!(rig.client.retained_event_count(), 0);
    let duplicate = rig.receive(event(rig.cookie, 0), Default::default());
    assert_eq!(duplicate.events_duplicate, 1);
    assert!(rig.client.drain_events().is_empty());
}
#[test]
fn full_delivery_capacity_refuses_unretained_receipt_and_later_retries() {
    let mut rig = Rig::new();
    rig.join();
    for sequence in 0..512 {
        rig.receive(event(rig.cookie, sequence), Default::default());
        rig.up.clear();
    }
    assert_eq!(rig.client.pending_event_count(), 512);
    let report = rig.receive(event(rig.cookie, 512), Default::default());
    assert_eq!(report.events_refused, 1);
    assert_eq!(report.datagrams_accepted, 0);
    assert_eq!(rig.client.retained_event_count(), 0);
    assert_eq!(rig.client.drain_events().len(), 512);
    let report = rig.receive(event(rig.cookie, 512), Default::default());
    assert_eq!(report.events_retained, 1);
    assert_eq!(rig.client.drain_events().len(), 1);
    assert!(matches!(
        decode_client(rig.up.back().unwrap()),
        Ok(ClientPacket::EventAck { sequence: 512, .. })
    ));
}
#[test]
fn retained_ack_tail_is_bounded_and_retries_without_a_new_packet() {
    let mut rig = Rig::new();
    rig.join();
    for sequence in 0..EVENT_LIMIT as u64 {
        let report = rig.receive(event(rig.cookie, sequence), ZERO);
        assert_eq!(report.events_retained, 1);
    }
    let report = rig.receive(event(rig.cookie, EVENT_LIMIT as u64), ZERO);
    assert_eq!(report.events_refused, 1);
    assert_eq!(report.acknowledgements_pending, EVENT_LIMIT);
    let mut acks = Vec::new();
    for _ in 0..2 {
        let report = rig
            .client
            .poll(rig.now, Default::default(), |b| {
                acks.push(decode_client(b).unwrap());
                true
            })
            .unwrap();
        assert_eq!(report.datagrams_accepted, 128);
    }
    assert_eq!(acks.len(), EVENT_LIMIT);
    for (sequence, ack) in acks.into_iter().enumerate() {
        assert_eq!(
            ack,
            ClientPacket::EventAck {
                session: rig.cookie,
                sequence: sequence as u64
            }
        );
    }
    assert_eq!(rig.client.last_report().acknowledgements_pending, 0);
}
#[test]
fn reconnect_rejects_old_welcome_snapshot_event_pong_and_command_ack() {
    let mut rig = Rig::new();
    rig.join();
    let old = rig.cookie;
    let old_probe = rig
        .client
        .clock_diagnostics(rig.now)
        .unwrap()
        .pending_probes
        .into_iter()
        .flatten()
        .next();
    rig.tick_client(InputFrame {
        buttons: BUTTON_FIRE,
        ..Default::default()
    });
    rig.receive(event(old, 0), ZERO);
    rig.up.clear();
    rig.client.reconnect(11, rig.now).unwrap();
    assert_eq!(rig.client.pending_input_count(), 0);
    assert_eq!(rig.client.pending_event_count(), 0);
    assert_eq!(rig.client.retained_event_count(), 0);
    assert_eq!(rig.client.retained_baseline_count(), 0);
    assert!(
        rig.client
            .clock_diagnostics(rig.now)
            .unwrap()
            .pending_probes
            .iter()
            .all(Option::is_none)
    );
    let report = rig.receive(
        ServerPacket::Welcome {
            nonce: 10,
            session: old,
            player_id: 0,
            tick: 0,
        },
        Default::default(),
    );
    assert_eq!(report.packets_rejected, 1);
    assert!(!rig.client.is_connected());
    rig.now = ms(300);
    rig.poll();
    rig.pump();
    let prior = rig.client.local_player().unwrap().clone();
    let rejected = rig.client.rejected_packets;
    rig.receive(
        ServerPacket::Welcome {
            nonce: 10,
            session: old,
            player_id: 0,
            tick: 0,
        },
        Default::default(),
    );
    rig.receive(event(old, 0), Default::default());
    rig.receive(
        ServerPacket::Snapshot(Snapshot {
            session: old,
            ..rig.snapshot(10)
        }),
        Default::default(),
    );
    rig.receive(
        ServerPacket::Extension(SX::CommandAck {
            session: old,
            sequence: 0,
        }),
        Default::default(),
    );
    let (nonce, client_us) = old_probe.map_or((1, 0), |p| (p.nonce, p.client_us));
    rig.receive(
        ServerPacket::Extension(SX::Pong {
            session: old,
            nonce,
            client_us,
            receive_us: 0,
            send_us: 0,
            tick: 0,
        }),
        Default::default(),
    );
    assert_eq!(rig.client.rejected_packets, rejected + 5);
    assert_eq!(rig.client.local_player(), Some(&prior));
    assert!(rig.client.drain_events().is_empty());
    // Even a matching handshake nonce cannot replace its already bound session.
    let report = rig.receive(
        ServerPacket::Welcome {
            nonce: 11,
            session: old,
            player_id: 0,
            tick: 0,
        },
        Default::default(),
    );
    assert_eq!(report.packets_rejected, 1);
}
#[test]
fn timeout_requires_fresh_host_nonce_and_old_welcome_cannot_resurrect() {
    let mut rig = Rig::new();
    rig.join();
    rig.now = ms(5000);
    assert!(!rig.poll().reconnect_required);
    rig.now = ms(5001);
    let report = rig.poll();
    assert!(report.reconnect_required);
    assert!(!report.tick_executed);
    assert!(!rig.client.is_connected());
    let report = rig.receive(
        ServerPacket::Welcome {
            nonce: 10,
            session: rig.cookie,
            player_id: 0,
            tick: 0,
        },
        Default::default(),
    );
    assert_eq!(report.packets_rejected, 1);
    assert!(matches!(
        rig.client.reconnect(10, rig.now),
        Err(DriverError::InvalidNonce)
    ));
    rig.client.reconnect(11, rig.now).unwrap();
    rig.poll();
    rig.up.clear();
    assert!(!rig.client.needs_reconnect());
}
#[test]
fn malformed_missing_delta_and_wrong_reconstructed_session_never_publish_or_ack() {
    let mut rig = Rig::new();
    rig.join();
    let before = rig.client.local_player().unwrap().clone();
    let full = encode_server(&ServerPacket::Snapshot(rig.snapshot(1))).unwrap();
    let mut next = full.clone();
    next[10] ^= 1; // Actual full codec below checks complete fields.
    let delta = ferravex_net::delta::Delta::between(99, &full, 100, &next).unwrap();
    let report = rig.receive(
        ServerPacket::Extension(SX::SnapshotDelta {
            session: rig.cookie,
            delta,
        }),
        Default::default(),
    );
    assert_eq!(report.snapshots_installed, 0);
    assert_eq!(rig.client.server_tick(), 0);
    assert_eq!(rig.client.local_player(), Some(&before));
    assert!(rig.up.iter().any(|b| matches!(
        decode_client(b),
        Ok(ClientPacket::Extension(CX::SnapshotAck { tick: None, .. }))
    )));
    rig.up.clear();
    let report = rig
        .client
        .receive(
            &[0; MAX_PACKET_BYTES + 1],
            rig.now,
            Default::default(),
            |_| panic!("malformed output"),
        )
        .unwrap();
    assert_eq!(report.packets_rejected, 1);
    let base = encode_server(&ServerPacket::Snapshot(rig.snapshot(0))).unwrap();
    let wrong = encode_server(&ServerPacket::Snapshot(Snapshot {
        session: rig.cookie + 1,
        ..rig.snapshot(2)
    }))
    .unwrap();
    let delta = ferravex_net::delta::Delta::between(0, &base, 2, &wrong).unwrap();
    let report = rig.receive(
        ServerPacket::Extension(SX::SnapshotDelta {
            session: rig.cookie,
            delta,
        }),
        ZERO,
    );
    assert_eq!(report.packets_rejected, 1);
    assert_eq!(report.snapshots_installed, 0);
    assert_eq!(rig.client.server_tick(), 0);
    assert_eq!(rig.client.retained_baseline_count(), 1);
}
#[test]
fn tactical_commands_use_admission_and_sequence_session_gates() {
    let mut rig = Rig::new();
    rig.server
        .enable_tactical(TacticalOptions {
            players: 2,
            ..Default::default()
        })
        .unwrap();
    rig.server.game.spawn_player(1); // Host's legitimate local participant.
    rig.join();
    rig.tick_server();
    rig.pump();
    assert!(rig.client.tactical_view().is_some());
    let action = ferravex_play::rounds::Action::BuyArmor;
    assert_eq!(rig.client.queue_action(action).unwrap(), 0);
    assert_eq!(rig.client.queue_action(action).unwrap(), 1);
    let refused = rig
        .client
        .poll(rig.now, Default::default(), |_| false)
        .unwrap();
    assert_eq!(refused.commands_remaining_due, 2);
    let mut offered = 0;
    let prefix = rig
        .client
        .poll(rig.now, Default::default(), |b| {
            offered += 1;
            if offered == 2 {
                false
            } else {
                admit(&mut rig.up, b)
            }
        })
        .unwrap();
    assert_eq!(prefix.commands_remaining_due, 1);
    let retry = rig.poll();
    assert_eq!(retry.commands_remaining_due, 0);
    assert_eq!(rig.up.len(), 2);
    rig.up.make_contiguous().reverse(); // Actual commands reordered before authority admission.
    rig.pump();
    rig.server
        .poll(rig.now, Default::default(), |_, b| admit(&mut rig.down, b))
        .unwrap();
    rig.pump();
    assert_eq!(rig.client.pending_command_count(), 0);
    assert_eq!(rig.client.last_action_result().unwrap().0, 1);
    assert!(rig.server.tactical_view().unwrap().player(0).unwrap().armor > 0);
    rig.client.reconnect(11, rig.now).unwrap();
    assert!(matches!(
        rig.client.queue_action(action),
        Err(DriverError::NotConnected)
    ));
}
#[test]
fn supplied_time_rejection_is_atomic_and_does_not_execute_tick() {
    let mut client = ClientState::new(arena(), 1).unwrap();
    client
        .poll(ms(10), ZERO, |_| panic!("zero output"))
        .unwrap();
    for now in [
        ms(9),
        Duration::from_nanos(10_000_001),
        Duration::from_micros((1 << 53) + 1),
    ] {
        assert!(
            client
                .tick(InputFrame::default(), now, Default::default(), |_| panic!(
                    "invalid time output"
                ))
                .is_err()
        );
        assert!(!client.last_report().tick_executed);
        assert_eq!(client.last_report().errors, 1);
        assert_eq!(client.pending_input_count(), 0);
    }
    assert!(matches!(
        ClientState::new(arena(), 0),
        Err(DriverError::InvalidNonce)
    ));
    assert!(matches!(
        client.reconnect(0, ms(10)),
        Err(DriverError::InvalidNonce)
    ));
    assert!(matches!(
        client.reconnect(1, ms(10)),
        Err(DriverError::InvalidNonce)
    ));
    client.reconnect(2, ms(10)).unwrap();
}

#[test]
fn accepted_combat_events_survive_loss_reordering_and_exact_once_delivery() {
    let mut rig = Rig::new();
    rig.server.game.spawn_player(1); // Legitimate local authority participant.
    rig.join();
    rig.tick_client(InputFrame {
        buttons: BUTTON_FIRE,
        ..Default::default()
    });
    rig.pump();
    let accepted = rig
        .server
        .tick(&[], rig.now, Default::default(), |_, b| {
            admit(&mut rig.down, b)
        })
        .unwrap();
    assert!(!accepted.events.is_empty());
    let mut events = Vec::new();
    while let Some(bytes) = rig.down.pop_front() {
        if matches!(
            decode_server(&bytes),
            Ok(ServerPacket::ReliableEvent { .. })
        ) {
            events.push(bytes);
        } else {
            rig.client
                .receive(&bytes, rig.now, Default::default(), |b| {
                    admit(&mut rig.up, b)
                })
                .unwrap();
        }
    }
    assert_eq!(events.len(), accepted.events.len());
    assert!(events.len() > 1);
    // Drop the admitted first event; deliver its actual successors backwards.
    for bytes in events.iter().skip(1).rev() {
        rig.client
            .receive(bytes, rig.now, Default::default(), |b| {
                admit(&mut rig.up, b)
            })
            .unwrap();
    }
    assert!(rig.client.drain_events().is_empty());
    rig.pump();
    assert_eq!(rig.server.pending_event_count(), 1);
    rig.now += ms(100);
    rig.server
        .poll(rig.now, Default::default(), |_, b| admit(&mut rig.down, b))
        .unwrap();
    rig.pump();
    let delivered = rig.client.drain_events();
    assert_eq!(
        delivered.iter().map(|e| e.event).collect::<Vec<_>>(),
        accepted.events
    );
    assert!(delivered.iter().all(|e| e.tick == rig.server.game.tick));
    rig.client
        .receive(&events[0], rig.now, Default::default(), |b| {
            admit(&mut rig.up, b)
        })
        .unwrap();
    rig.pump();
    assert!(rig.client.drain_events().is_empty());
    assert_eq!(rig.server.pending_event_count(), 0);
}
