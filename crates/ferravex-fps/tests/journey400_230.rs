//! Socket-free production authority plus the existing v5 codec. The public
//! adapter's Deref target reaches the registered core without a root reexport.
//! This fixture is deliberately not GameClient extraction or mock simulation.
use ferravex_fps::tactical::{SessionEvent, encode_action};
use ferravex_fps::{Arena, BUTTON_FIRE, GameEvent, GameServer, InputFrame};
use ferravex_math::Vec3;
use ferravex_net::baselines::{BaselineLimits, ReceiverBaselines};
use ferravex_net::reliable::{EVENT_LIMIT, EventPacket};
use ferravex_net::{
    ClientExtension as CX, ClientPacket, MAX_PACKET_BYTES, MAX_QUEUED_INPUTS,
    ServerExtension as SX, ServerPacket, Snapshot, TimedInput, decode_server, encode_client,
    encode_server,
};
use ferravex_play::rounds::{Action, Phase};
use std::{net::SocketAddr, ops::Deref, time::Duration};
type Authority = <GameServer as Deref>::Target;
type Wire = Vec<(SocketAddr, Vec<u8>)>;

fn arena() -> Arena {
    Arena {
        solids: vec![],
        spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -6.)],
    }
}
fn addr(port: u16) -> SocketAddr {
    ([127, 0, 0, 1], port).into()
}
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn step(
    state: &mut Authority,
    now: Duration,
    local: &[(u8, InputFrame)],
) -> (Vec<GameEvent>, Wire) {
    let mut wire = Vec::new();
    let result = state
        .tick(local, now, Default::default(), |to, bytes| {
            wire.push((to, bytes.to_vec()));
            true
        })
        .unwrap();
    assert!(result.report.tick_executed);
    assert_eq!(result.report.datagrams_accepted, wire.len());
    (result.events, wire)
}
fn receive(
    state: &mut Authority,
    peer: SocketAddr,
    packet: ClientPacket,
    now: Duration,
    cookie: u64,
) -> Wire {
    let mut wire = Vec::new();
    let bytes = encode_client(&packet).unwrap();
    let report = state
        .receive(
            peer,
            &bytes,
            now,
            || cookie,
            Default::default(),
            |to, bytes| {
                assert!(bytes.len() <= MAX_PACKET_BYTES);
                wire.push((to, bytes.to_vec()));
                true
            },
        )
        .unwrap();
    assert_eq!(report.datagrams_accepted, wire.len());
    assert_eq!(report.packets_received, 1);
    wire
}
fn poll(state: &mut Authority, now: Duration) -> Wire {
    let mut wire = Vec::new();
    state
        .poll(now, Default::default(), |to, bytes| {
            wire.push((to, bytes.to_vec()));
            true
        })
        .unwrap();
    wire
}
struct ClientFixture {
    addr: SocketAddr,
    nonce: u64,
    session: u64,
    id: u8,
    retained: ReceiverBaselines,
    latest: Option<Snapshot>,
}
impl ClientFixture {
    fn join(state: &mut Authority, port: u16, now: Duration) -> Self {
        let mut fixture = Self {
            addr: addr(port),
            nonce: u64::from(port),
            session: 0,
            id: 255,
            retained: ReceiverBaselines::new(BaselineLimits {
                max_snapshots: 32,
                max_bytes: 32 * MAX_PACKET_BYTES,
            })
            .unwrap(),
            latest: None,
        };
        let wire = receive(
            state,
            fixture.addr,
            ClientPacket::Hello {
                nonce: fixture.nonce,
            },
            now,
            u64::from(port) + 1000,
        );
        let acks = fixture.accept(&wire);
        assert_ne!(fixture.session, 0);
        assert!(fixture.latest.is_some());
        for ack in acks {
            receive(state, fixture.addr, ack, now, 999);
        }
        fixture
    }
    // Validate session/tick/membership, then install exact full bytes before
    // emitting an application snapshot ACK. A delta with missing history asks
    // for a full frame and never changes published state or authorizes a tick.
    fn accept(&mut self, wire: &Wire) -> Vec<ClientPacket> {
        let mut acks = Vec::new();
        for (peer, bytes) in wire {
            if *peer != self.addr {
                continue;
            }
            match decode_server(bytes).unwrap() {
                ServerPacket::Welcome {
                    nonce,
                    session,
                    player_id,
                    ..
                } => {
                    assert_eq!(nonce, self.nonce);
                    if self.session != session {
                        self.retained.reset();
                        self.latest = None;
                    }
                    self.session = session;
                    self.id = player_id;
                }
                ServerPacket::Snapshot(snapshot) => self.install(snapshot, bytes, &mut acks),
                ServerPacket::Extension(SX::SnapshotDelta { session, delta }) => {
                    if session != self.session {
                        continue;
                    }
                    let mut full = [0; MAX_PACKET_BYTES];
                    match self.retained.apply_into(&delta, &mut full) {
                        Ok(n) => {
                            let ServerPacket::Snapshot(snapshot) =
                                decode_server(&full[..n]).unwrap()
                            else {
                                panic!("full snapshot");
                            };
                            assert_eq!(snapshot.tick, delta.tick);
                            assert_eq!(snapshot.session, session);
                            self.install(snapshot, &full[..n], &mut acks);
                        }
                        Err(_) => acks.push(ClientPacket::Extension(CX::SnapshotAck {
                            session,
                            tick: None,
                        })),
                    }
                }
                ServerPacket::ReliableEvent { session, event } => {
                    assert_eq!(session, self.session);
                    SessionEvent::decode(&event.payload).unwrap();
                }
                ServerPacket::Extension(_) => {}
            }
        }
        acks
    }
    fn install(&mut self, snapshot: Snapshot, bytes: &[u8], acks: &mut Vec<ClientPacket>) {
        if snapshot.session != self.session {
            return;
        }
        assert!(snapshot.players.iter().any(|p| p.id == self.id));
        self.retained.install(snapshot.tick, bytes).unwrap();
        assert_eq!(
            encode_server(&ServerPacket::Snapshot(snapshot.clone())).unwrap(),
            bytes
        );
        let tick = snapshot.tick;
        self.latest = Some(snapshot);
        acks.push(ClientPacket::Extension(CX::SnapshotAck {
            session: self.session,
            tick: Some(tick),
        }));
    }
    fn command(&self, sequence: u64, action: Action) -> ClientPacket {
        ClientPacket::Extension(CX::Command {
            session: self.session,
            event: EventPacket {
                sequence,
                payload: encode_action(action).to_vec(),
            },
        })
    }
    fn inputs(&self, sequence: u32, buttons: u8, shot_tick: Option<u32>) -> ClientPacket {
        ClientPacket::Extension(CX::Inputs {
            session: self.session,
            inputs: vec![TimedInput {
                input: InputFrame {
                    sequence,
                    buttons,
                    move_z: 1.,
                    ..Default::default()
                },
                shot_tick,
            }],
        })
    }
}
fn gameplay(wire: &Wire) -> Vec<(u64, SessionEvent)> {
    wire.iter()
        .filter_map(|(_, b)| match decode_server(b).unwrap() {
            ServerPacket::ReliableEvent { event, .. } => Some((
                event.sequence,
                SessionEvent::decode(&event.payload).unwrap(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn supplied_time_hello_throttle_retry_rehello_reset_and_exact_timeout() {
    let mut state = Authority::new(arena());
    let mut client = ClientFixture::join(&mut state, 9000, ms(0));
    let prior = state.game.player(0).unwrap().clone();
    let old_session = client.session;
    let mut cookie_calls = 0;
    let hello = encode_client(&ClientPacket::Hello {
        nonce: client.nonce + 1,
    })
    .unwrap();
    let report = state
        .receive(
            client.addr,
            &hello,
            ms(249),
            || {
                cookie_calls += 1;
                88
            },
            Default::default(),
            |_, _| panic!("throttled"),
        )
        .unwrap();
    assert_eq!(cookie_calls, 0);
    assert_eq!(report.datagrams_accepted, 0);
    client.nonce += 1;
    let wire = receive(
        &mut state,
        client.addr,
        ClientPacket::Hello {
            nonce: client.nonce,
        },
        ms(250),
        88,
    );
    assert_eq!(state.game.player(0).unwrap(), &prior);
    client.accept(&wire);
    assert_eq!(client.session, 88);
    assert_ne!(client.session, old_session);
    // Old-session input, ACK, ping and commands never enter new-session state.
    for packet in [
        ClientPacket::Input {
            session: old_session,
            inputs: vec![InputFrame::default()],
        },
        ClientPacket::EventAck {
            session: old_session,
            sequence: 0,
        },
        ClientPacket::Extension(CX::SnapshotAck {
            session: old_session,
            tick: Some(0),
        }),
        ClientPacket::Extension(CX::Ping {
            session: old_session,
            nonce: 1,
            client_us: 0,
        }),
        ClientPacket::Extension(CX::Command {
            session: old_session,
            event: EventPacket {
                sequence: 0,
                payload: encode_action(Action::Cancel).to_vec(),
            },
        }),
    ] {
        receive(&mut state, client.addr, packet, ms(250), 99);
        assert_eq!(state.last_report().packets_rejected, 1);
    }
    state
        .poll(ms(5250), Default::default(), |_, _| true)
        .unwrap();
    assert_eq!(
        state.peer_count(),
        1,
        "timeout is strictly greater than five seconds"
    );
    state
        .poll(ms(5251), Default::default(), |_, _| true)
        .unwrap();
    assert_eq!(state.last_report().peers_timed_out, 1);
    assert_eq!(state.peer_count(), 0);
    assert!(state.game.player(0).is_none());
    let replacement = ClientFixture::join(&mut state, 9001, ms(5251));
    assert_eq!(replacement.id, 0);
    assert_eq!(state.game.player(0).unwrap().health, 100);
}

#[test]
fn dropped_welcome_retries_same_cookie_then_fresh_hello_preserves_real_player_state() {
    let mut state = Authority::new(arena());
    let hello = encode_client(&ClientPacket::Hello { nonce: 1 }).unwrap();
    let mut calls = 0;
    let report = state
        .receive(
            addr(9000),
            &hello,
            ms(0),
            || {
                calls += 1;
                7
            },
            Default::default(),
            |_, _| true,
        )
        .unwrap();
    assert_eq!(report.datagrams_accepted, 2); // caller intentionally drops accepted output
    let mut wire = Vec::new();
    state
        .receive(
            addr(9000),
            &hello,
            ms(250),
            || panic!("same session"),
            Default::default(),
            |to, b| {
                wire.push((to, b.to_vec()));
                true
            },
        )
        .unwrap();
    assert_eq!(calls, 1);
    assert!(matches!(
        decode_server(&wire[0].1).unwrap(),
        ServerPacket::Welcome { session: 7, .. }
    ));
    // Use ordinary accepted gameplay, rather than writing a fabricated state.
    receive(
        &mut state,
        addr(9000),
        ClientPacket::Input {
            session: 7,
            inputs: vec![InputFrame {
                buttons: BUTTON_FIRE,
                move_x: 1.,
                ..Default::default()
            }],
        },
        ms(250),
        99,
    );
    let events = step(&mut state, ms(250), &[]).0;
    assert!(events.iter().any(|e| matches!(e, GameEvent::Shot { .. })));
    let prior = state.game.player(0).unwrap().clone();
    assert!(prior.ammo < ferravex_fps::MAGAZINE_SIZE);
    receive(
        &mut state,
        addr(9000),
        ClientPacket::Hello { nonce: 2 },
        ms(500),
        8,
    );
    assert_eq!(state.game.player(0).unwrap(), &prior);
    assert_eq!(state.pending_event_count(), 0);
}

#[test]
fn invalid_backwards_time_and_zero_cookie_reject_without_effects() {
    let mut state = Authority::new(arena());
    state
        .poll(ms(100), Default::default(), |_, _| true)
        .unwrap();
    for time in [ms(99), Duration::from_nanos(100_000_001), Duration::MAX] {
        let before = state.game.state_hash().unwrap();
        assert!(
            state
                .tick(&[], time, Default::default(), |_, _| panic!(
                    "invalid time output"
                ))
                .is_err()
        );
        assert_eq!(state.last_report().errors, 1);
        assert!(!state.last_report().tick_executed);
        assert_eq!(state.game.state_hash().unwrap(), before);
    }
    let hello = encode_client(&ClientPacket::Hello { nonce: 1 }).unwrap();
    assert!(
        state
            .receive(
                addr(9000),
                &hello,
                ms(100),
                || 0,
                Default::default(),
                |_, _| panic!("invalid cookie")
            )
            .is_err()
    );
    assert_eq!(state.peer_count(), 0);
    assert!(state.game.players.is_empty());
    let mut oversize = vec![0; MAX_PACKET_BYTES + 1];
    oversize[..hello.len()].copy_from_slice(&hello);
    assert_eq!(
        state
            .receive(
                addr(9000),
                &oversize,
                ms(100),
                || panic!("malformed"),
                Default::default(),
                |_, _| true
            )
            .unwrap()
            .packets_rejected,
        1
    );
    ClientFixture::join(&mut state, 9000, ms(100));
    assert_eq!(state.peer_count(), 1);
}

#[test]
fn refusal_retains_reliable_head_and_tail_until_exact_accepted_retry_time() {
    let mut state = Authority::new(arena());
    let client = ClientFixture::join(&mut state, 9000, ms(0));
    receive(
        &mut state,
        client.addr,
        client.inputs(0, BUTTON_FIRE, None),
        ms(0),
        99,
    );
    // Tick commits and generates a shot; refusal never becomes a local send.
    let mut offers = 0;
    let result = state
        .tick(&[], ms(0), Default::default(), |_, _| {
            offers += 1;
            false
        })
        .unwrap();
    assert!(result.report.tick_executed);
    assert_eq!(offers, 1);
    assert_eq!(result.report.datagrams_refused, 1);
    assert_eq!(result.report.datagrams_accepted, 0);
    assert_eq!(
        result.report.reliable_remaining_due,
        state.pending_event_count()
    );
    let sent = poll(&mut state, ms(0));
    assert_eq!(gameplay(&sent).len(), 1);
    assert_eq!(state.last_report().datagrams_accepted, 1);
    assert_eq!(state.last_report().reliable_remaining_due, 0);
    assert!(poll(&mut state, ms(99)).is_empty());
    let mut refused = 0;
    state
        .poll(ms(100), Default::default(), |_, _| {
            refused += 1;
            false
        })
        .unwrap();
    assert_eq!(refused, 1);
    let retry = poll(&mut state, ms(100));
    assert_eq!(retry, sent, "refusal did not advance the due timestamp");
    assert!(poll(&mut state, ms(199)).is_empty());
    assert_eq!(poll(&mut state, ms(200)), sent);
    receive(
        &mut state,
        client.addr,
        ClientPacket::EventAck {
            session: client.session,
            sequence: 0,
        },
        ms(200),
        99,
    );
    assert_eq!(state.pending_event_count(), 0);
}

#[test]
fn flood_redundancy_input_admission_is_bounded_and_only_tick_advances_commands() {
    let mut state = Authority::new(arena());
    let client = ClientFixture::join(&mut state, 9000, ms(0));
    for sequence in 0..MAX_QUEUED_INPUTS as u32 + 5 {
        receive(
            &mut state,
            client.addr,
            client.inputs(sequence, 0, None),
            ms(0),
            99,
        );
        assert_eq!(state.game.tick, 0);
        assert_eq!(
            state.last_report().inputs_accepted,
            usize::from((sequence as usize) < MAX_QUEUED_INPUTS)
        );
        assert_eq!(
            state.last_report().inputs_full,
            usize::from((sequence as usize) >= MAX_QUEUED_INPUTS)
        );
    }
    let before = state.game.player(0).unwrap().position;
    poll(&mut state, ms(0));
    assert_eq!(state.game.player(0).unwrap().position, before);
    let _ = step(&mut state, ms(0), &[]);
    assert_eq!(state.game.tick, 1);
    assert!(state.game.player(0).unwrap().position.distance(before) < 1.);
    let last = MAX_QUEUED_INPUTS as u32 - 1;
    receive(
        &mut state,
        client.addr,
        client.inputs(last, 0, None),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().inputs_duplicate, 1);
    receive(
        &mut state,
        client.addr,
        client.inputs(last - 1, 0, None),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().inputs_out_of_order, 1);
    receive(
        &mut state,
        client.addr,
        client.inputs(last + 1, 0, None),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().inputs_accepted, 1);
    // Server consumes coalesced input once per simulation tick, not per packet.
    for _ in 0..20 {
        step(&mut state, ms(0), &[]);
    }
    let position = state.game.player(0).unwrap().position;
    for _ in 0..20 {
        step(&mut state, ms(0), &[]);
    }
    assert_eq!(
        state.game.player(0).unwrap().position,
        position,
        "held input expires after twelve ticks"
    );
}

#[test]
fn local_authority_input_is_validated_and_real_shot_claims_emit_only_accepted_events() {
    let mut state = Authority::new(arena());
    state.game.spawn_player(1);
    let client = ClientFixture::join(&mut state, 9000, ms(0));
    for local in [
        vec![(0, InputFrame::default())],
        vec![(2, InputFrame::default())],
        vec![(1, InputFrame::default()); 2],
    ] {
        assert!(
            state
                .tick(&local, ms(0), Default::default(), |_, _| panic!(
                    "invalid local"
                ))
                .is_err()
        );
        assert_eq!(state.game.tick, 0);
    }
    // Actual local movement produces history, then a historical remote shot hits.
    for _ in 0..15 {
        step(
            &mut state,
            ms(0),
            &[(
                1,
                InputFrame {
                    move_x: 1.,
                    ..Default::default()
                },
            )],
        );
    }
    assert!(state.game.player(1).unwrap().position.x > 0.8);
    receive(
        &mut state,
        client.addr,
        client.inputs(0, BUTTON_FIRE, Some(0)),
        ms(0),
        99,
    );
    let (events, wire) = step(&mut state, ms(0), &[]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Hit { victim: 1, .. }))
    );
    assert_eq!(state.game.player(1).unwrap().health, 0);
    assert!(
        gameplay(&wire)
            .iter()
            .any(|(_, e)| matches!(e, SessionEvent::Gameplay(_)))
    );
    // A future claim suppresses the shot permanently until a new input arrives.
    let future = state.game.tick + 1;
    receive(
        &mut state,
        client.addr,
        client.inputs(1, BUTTON_FIRE, Some(future)),
        ms(0),
        99,
    );
    let ammo = state.game.player(0).unwrap().ammo;
    assert!(step(&mut state, ms(0), &[]).0.is_empty());
    assert_eq!(state.rejected_shots, 1);
    for _ in 0..20 {
        step(&mut state, ms(0), &[]);
    }
    assert_eq!(state.game.player(0).unwrap().ammo, ammo);
}

#[test]
fn exact_snapshot_ack_delta_missing_baseline_full_recovery_and_eviction() {
    let mut state = Authority::new(arena());
    let mut client = ClientFixture::join(&mut state, 9000, ms(0));
    let mut wire = Vec::new();
    for _ in 0..3 {
        wire = step(&mut state, ms(0), &[]).1;
    }
    assert!(wire.iter().any(|(_, b)| matches!(
        decode_server(b).unwrap(),
        ServerPacket::Extension(SX::SnapshotDelta { .. })
    )));
    let acks = client.accept(&wire);
    assert_eq!(client.latest.as_ref().unwrap().tick, 3);
    for ack in acks {
        receive(&mut state, client.addr, ack, ms(0), 99);
    }
    receive(
        &mut state,
        client.addr,
        ClientPacket::Extension(CX::SnapshotAck {
            session: client.session,
            tick: Some(999),
        }),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().packets_rejected, 1);
    client.retained.reset();
    for _ in 0..3 {
        wire = step(&mut state, ms(0), &[]).1;
    }
    let requests = client.accept(&wire);
    assert_eq!(client.latest.as_ref().unwrap().tick, 3);
    assert_eq!(requests.len(), 1);
    assert!(matches!(
        requests[0],
        ClientPacket::Extension(CX::SnapshotAck { tick: None, .. })
    ));
    for request in requests {
        receive(&mut state, client.addr, request, ms(0), 99);
    }
    for _ in 0..3 {
        wire = step(&mut state, ms(0), &[]).1;
    }
    assert!(
        wire.iter()
            .any(|(_, b)| matches!(decode_server(b).unwrap(), ServerPacket::Snapshot(_)))
    );
    for ack in client.accept(&wire) {
        receive(&mut state, client.addr, ack, ms(0), 99);
    }
    assert_eq!(client.latest.as_ref().unwrap().tick, 9);
    // ACKed history is bounded; after 32 new frames it is evicted and full
    // fallback recovers even with no further receiver acknowledgments.
    let mut saw_full = false;
    for _ in 0..99 {
        wire = step(&mut state, ms(0), &[]).1;
        saw_full |= wire
            .iter()
            .any(|(_, b)| matches!(decode_server(b).unwrap(), ServerPacket::Snapshot(_)));
    }
    assert!(saw_full);
    let acks = client.accept(&wire);
    assert_eq!(client.latest.as_ref().unwrap().tick, state.game.tick);
    for ack in acks {
        receive(&mut state, client.addr, ack, ms(0), 99);
    }
    for _ in 0..3 {
        wire = step(&mut state, ms(0), &[]).1;
    }
    assert!(wire.iter().any(|(_, b)| matches!(
        decode_server(b).unwrap(),
        ServerPacket::Extension(SX::SnapshotDelta { .. })
    )));
}

#[test]
fn refused_snapshot_is_not_authorized_by_an_exact_tick_ack() {
    let mut state = Authority::new(arena());
    let client = ClientFixture::join(&mut state, 9000, ms(0));
    step(&mut state, ms(0), &[]);
    step(&mut state, ms(0), &[]);
    let tick = state
        .tick(&[], ms(0), Default::default(), |_, _| false)
        .unwrap();
    assert_eq!(tick.report.datagrams_refused, 1);
    assert_eq!(state.game.tick, 3);
    receive(
        &mut state,
        client.addr,
        ClientPacket::Extension(CX::SnapshotAck {
            session: client.session,
            tick: Some(3),
        }),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().packets_rejected, 1);
    let mut wire = Vec::new();
    for _ in 0..3 {
        wire = step(&mut state, ms(0), &[]).1;
    }
    let delta = wire
        .iter()
        .find_map(|(_, b)| match decode_server(b).unwrap() {
            ServerPacket::Extension(SX::SnapshotDelta { delta, .. }) => Some(delta),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        delta.base_tick, 0,
        "only the retained tick zero ACK authorizes a base"
    );
}

#[test]
fn tactical_ready_ordered_durable_commands_capacity_and_poll_without_new_packets() {
    let mut state = Authority::new(arena());
    state.enable_tactical(Default::default()).unwrap();
    let a = ClientFixture::join(&mut state, 9000, ms(0));
    assert_eq!(state.tactical_lobby(), Some((1, 2)));
    assert!(state.tactical_view().is_none());
    let _b = ClientFixture::join(&mut state, 9001, ms(0));
    let (_, wire) = step(&mut state, ms(0), &[]);
    assert_eq!(state.tactical_view().unwrap().phase, Phase::Buy);
    assert!(
        gameplay(&wire)
            .iter()
            .any(|(_, e)| matches!(e, SessionEvent::Round(_)))
    );
    // Deliver command one twice ahead of zero. It is ACKed once retained, but
    // does not execute until zero arrives and result capacity was preflighted.
    receive(&mut state, a.addr, a.command(1, Action::Cancel), ms(0), 99);
    assert_eq!(state.last_report().commands_retained, 1);
    assert_eq!(state.last_report().commands_executed, 0);
    receive(&mut state, a.addr, a.command(1, Action::Cancel), ms(0), 99);
    assert_eq!(state.last_report().commands_duplicate, 1);
    receive(&mut state, a.addr, a.command(0, Action::Cancel), ms(0), 99);
    assert_eq!(state.last_report().commands_executed, 2);
    for seq in 2..(EVENT_LIMIT - 1) as u64 {
        receive(
            &mut state,
            a.addr,
            a.command(seq, Action::Cancel),
            ms(0),
            99,
        );
        assert_eq!(state.last_report().commands_executed, 1);
    }
    // One round event plus 255 results fill a's result window exactly.
    let view = state.tactical_view().unwrap();
    let money = view.player(a.id).unwrap().money;
    assert_eq!(view.player(a.id).unwrap().armor, 0);
    let command = a.command((EVENT_LIMIT - 1) as u64, Action::BuyArmor);
    let ack = receive(&mut state, a.addr, command.clone(), ms(0), 99);
    assert!(ack.iter().any(|(_, b)| matches!(
        decode_server(b).unwrap(),
        ServerPacket::Extension(SX::CommandAck { .. })
    )));
    assert_eq!(state.last_report().commands_executed, 0);
    assert_eq!(state.pending_command_count(), 1);
    assert_eq!(
        state.tactical_view().unwrap().player(a.id).unwrap().money,
        money
    );
    // ACK frees downstream space. No new command datagram is necessary.
    receive(
        &mut state,
        a.addr,
        ClientPacket::EventAck {
            session: a.session,
            sequence: 0,
        },
        ms(0),
        99,
    );
    poll(&mut state, ms(0));
    assert_eq!(state.last_report().commands_executed, 1);
    assert_eq!(state.pending_command_count(), 0);
    let view = state.tactical_view().unwrap();
    assert!(view.player(a.id).unwrap().money < money);
    assert_eq!(view.player(a.id).unwrap().armor, 100);
    let after = view.player(a.id).unwrap().money;
    receive(&mut state, a.addr, command, ms(0), 99);
    assert_eq!(state.last_report().commands_duplicate, 1);
    assert_eq!(
        state.tactical_view().unwrap().player(a.id).unwrap().money,
        after
    );
    state.restart_tactical();
    step(&mut state, ms(0), &[]); // Existing full-window slow-peer disconnect policy.
    assert!(state.slow_event_disconnects >= 1);
}

#[test]
fn whole_reliable_sequence_window_can_retain_successors_without_refusing_missing_head() {
    let mut state = Authority::new(arena());
    let client = ClientFixture::join(&mut state, 9000, ms(0));
    for sequence in 1..EVENT_LIMIT as u64 {
        receive(
            &mut state,
            client.addr,
            client.command(sequence, Action::Cancel),
            ms(0),
            99,
        );
        assert_eq!(state.last_report().commands_executed, 0);
    }
    assert_eq!(state.pending_command_count(), EVENT_LIMIT - 1);
    receive(
        &mut state,
        client.addr,
        client.command(0, Action::Cancel),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().commands_retained, 1);
    assert_eq!(state.last_report().commands_executed, 8);
    for _ in 0..31 {
        poll(&mut state, ms(0));
    }
    assert_eq!(state.pending_command_count(), 0);
    assert_eq!(state.pending_event_count(), EVENT_LIMIT);
    // New in-window command is durably held while result space is full.
    receive(
        &mut state,
        client.addr,
        client.command(EVENT_LIMIT as u64, Action::Cancel),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().commands_executed, 0);
    receive(
        &mut state,
        client.addr,
        client.command(EVENT_LIMIT as u64 * 2, Action::Cancel),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().commands_refused, 1);
    assert_eq!(state.last_report().packets_rejected, 1);
    assert_eq!(state.pending_command_count(), 1);
}

#[test]
fn redundant_controls_merge_fire_edge_once_without_an_extra_simulation_tick() {
    let mut state = Authority::new(arena());
    let client = ClientFixture::join(&mut state, 9000, ms(0));
    receive(
        &mut state,
        client.addr,
        client.inputs(0, BUTTON_FIRE, None),
        ms(0),
        99,
    );
    receive(
        &mut state,
        client.addr,
        client.inputs(1, 0, None),
        ms(0),
        99,
    );
    receive(
        &mut state,
        client.addr,
        client.inputs(1, 0, None),
        ms(0),
        99,
    );
    assert_eq!(state.last_report().inputs_duplicate, 1);
    let events = step(&mut state, ms(0), &[]).0;
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, GameEvent::Shot { .. }))
            .count(),
        1
    );
    assert_eq!(state.last_report().inputs_consumed, 1);
    assert_eq!(state.last_report().inputs_coalesced, 1);
    assert_eq!(state.last_report().authority_inputs, 1);
    receive(
        &mut state,
        client.addr,
        client.inputs(2, 0, None),
        ms(0),
        99,
    );
    for _ in 0..20 {
        assert!(step(&mut state, ms(0), &[]).0.is_empty());
    }
    assert_eq!(
        state.game.player(0).unwrap().ammo,
        ferravex_fps::MAGAZINE_SIZE - 1
    );
}
