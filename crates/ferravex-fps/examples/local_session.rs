//! Headless deterministic local network inspector using the production drivers.
//!
//! Run `cargo run -p ferravex-fps --example local_session --offline --locked -j 2
//! -- --ticks 256 [--impaired] [--reconnect-at 128]`.
//! `--help` describes the bounded scenario. This is scripted headless input,
//! not browser autoplay, interactive rendering, physical-device or audio evidence.
//! Each catch-up tick uses its exact 15,625us supplied-time boundary. Frame time
//! is preflighted before mutation; runtime driver errors terminate the inspector
//! with its committed prefix intact, never by retrying an already executed tick.
//! The normal arena and game rules are unchanged. Menu frames mask only local
//! commands; they continue polling, receiving and executing authority ticks.
use ferravex_fps::{
    Arena, BUTTON_FIRE, BUTTON_RELOAD, InputFrame, NetworkEvent,
    client_state::{ClientState, WorkReport},
    server_state::{OutputBudget, ServerState, TickOutput},
};
use ferravex_net::{
    MAX_PACKET_BYTES,
    local_link::{Endpoint, Impairment, Limits, LinkError, LocalLink},
};
use std::{error::Error, net::SocketAddr, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TICK_US: u64 = 15_625; // Exactly 64 Hz; no rounded 16ms simulation steps.
const MAX_FRAME_TICKS: u32 = 4;
const PUMP_PASSES: usize = 4;
const INGRESS_PER_ENDPOINT: usize = 16;
const BUDGET: OutputBudget = OutputBudget {
    datagrams: 8,
    bytes: 8 * MAX_PACKET_BYTES,
};
const SERVER: u64 = 0;
const CLIENTS: [u64; 2] = [1, 2];

fn peer(id: u64) -> SocketAddr {
    ([127, 0, 0, 1], 9000 + id as u16).into()
}
fn route(address: SocketAddr) -> Option<usize> {
    CLIENTS.iter().position(|&id| peer(id) == address)
}
fn offer(
    link: &mut LocalLink,
    from: Endpoint,
    to: Endpoint,
    bytes: &[u8],
    now: Duration,
    fault: &mut Option<LinkError>,
) -> bool {
    match link.send(from, to, bytes, now) {
        Ok(()) => true,
        Err(LinkError::PacketCapacity | LinkError::ByteCapacity) => false,
        Err(error) => {
            *fault = Some(error);
            false
        }
    }
}
fn fault(error: Option<LinkError>) -> Result<()> {
    if let Some(error) = error {
        Err(error.into())
    } else {
        Ok(())
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Totals {
    ingress: usize,
    lost: usize,
    refused: usize,
    snapshots: usize,
    replayed_inputs: usize,
    acknowledged_inputs: usize,
    prediction_ticks: [usize; 2],
    accepted_events: usize,
    delivered_events: [usize; 2],
    frames: usize,
    zero_tick_frames: usize,
    issued_ticks: u64,
    executed_ticks: u64,
}
#[derive(Default)]
struct FrameReport {
    issued: u32,
    executed: u32,
    accepted: Vec<NetworkEvent>,
    delivered: [Vec<NetworkEvent>; 2],
}
struct LocalSession {
    server: ServerState,
    clients: [ClientState; 2],
    link: LocalLink,
    authority: Endpoint,
    endpoints: [Endpoint; 2],
    now: Duration,
    accumulator_us: u64,
    next_nonce: u64,
    cookie: u64,
    totals: Totals,
}
impl LocalSession {
    fn new(link_id: u64, arena: Arena, impairment: Impairment, limits: Limits) -> Result<Self> {
        let link = LocalLink::new(
            link_id,
            &[SERVER, CLIENTS[0], CLIENTS[1]],
            limits,
            impairment,
        )?;
        let authority = link.endpoint(SERVER)?;
        let endpoints = [link.endpoint(CLIENTS[0])?, link.endpoint(CLIENTS[1])?];
        Ok(Self {
            server: ServerState::new(arena.clone()),
            clients: [
                ClientState::new(arena.clone(), 1)?,
                ClientState::new(arena, 2)?,
            ],
            link,
            authority,
            endpoints,
            now: Duration::ZERO,
            accumulator_us: 0,
            next_nonce: 3,
            cookie: 100,
            totals: Totals::default(),
        })
    }
    fn observe(&mut self, i: usize, report: WorkReport) {
        self.totals.refused += report.datagrams_refused;
        self.totals.snapshots += report.snapshots_installed;
        self.totals.replayed_inputs += report.replayed_inputs;
        self.totals.acknowledged_inputs += report.inputs_acknowledged;
        self.totals.prediction_ticks[i] += usize::from(report.tick_executed);
    }
    fn reconnect(&mut self, i: usize) -> Result<()> {
        let nonce = self.next_nonce;
        self.next_nonce = self
            .next_nonce
            .checked_add(1)
            .ok_or("host nonce exhausted")?;
        self.clients[i].reconnect(nonce, self.now)?;
        self.endpoints[i] = self.link.reset_endpoint(self.endpoints[i], self.now)?;
        Ok(())
    }
    fn poll(&mut self) -> Result<()> {
        for i in 0..2 {
            if self.clients[i].needs_reconnect() {
                self.reconnect(i)?;
            }
            let mut error = None;
            let report = self.clients[i].poll(self.now, BUDGET, |bytes| {
                offer(
                    &mut self.link,
                    self.endpoints[i],
                    self.authority,
                    bytes,
                    self.now,
                    &mut error,
                )
            })?;
            fault(error)?;
            self.observe(i, report);
        }
        let mut error = None;
        let report = self.server.poll(self.now, BUDGET, |to, bytes| {
            let Some(i) = route(to) else {
                return false;
            };
            offer(
                &mut self.link,
                self.authority,
                self.endpoints[i],
                bytes,
                self.now,
                &mut error,
            )
        })?;
        fault(error)?;
        self.totals.refused += report.datagrams_refused;
        Ok(())
    }
    fn pump(&mut self) -> Result<()> {
        // <=4*3*16=192 removals/driver receives per pump. Loss consumes a work
        // slot too. New callback output goes back through the same capacity gate.
        let mut bytes = [0; MAX_PACKET_BYTES];
        for _ in 0..PUMP_PASSES {
            for destination in 0..3 {
                let to = if destination == 0 {
                    self.authority
                } else {
                    self.endpoints[destination - 1]
                };
                for _ in 0..INGRESS_PER_ENDPOINT {
                    let Some(delivery) = self.link.receive(to, self.now, &mut bytes)? else {
                        break;
                    };
                    if delivery.lost {
                        self.totals.lost += 1;
                        continue;
                    }
                    self.totals.ingress += 1;
                    let mut error = None;
                    if destination == 0 {
                        let from = delivery.from.peer_id();
                        let report = self.server.receive(
                            peer(from),
                            &bytes[..delivery.len],
                            self.now,
                            || {
                                self.cookie += 1;
                                self.cookie
                            },
                            BUDGET,
                            |to, bytes| {
                                let Some(i) = route(to) else {
                                    return false;
                                };
                                offer(
                                    &mut self.link,
                                    self.authority,
                                    self.endpoints[i],
                                    bytes,
                                    self.now,
                                    &mut error,
                                )
                            },
                        )?;
                        self.totals.refused += report.datagrams_refused;
                    } else {
                        if delivery.from != self.authority {
                            return Err("unexpected local route".into());
                        }
                        let i = destination - 1;
                        let report = self.clients[i].receive(
                            &bytes[..delivery.len],
                            self.now,
                            BUDGET,
                            |bytes| {
                                offer(
                                    &mut self.link,
                                    self.endpoints[i],
                                    self.authority,
                                    bytes,
                                    self.now,
                                    &mut error,
                                )
                            },
                        )?;
                        self.observe(i, report);
                    }
                    fault(error)?;
                }
            }
        }
        Ok(())
    }
    fn tick(&mut self, inputs: [InputFrame; 2]) -> Result<TickOutput> {
        for (i, input) in inputs.into_iter().enumerate() {
            let mut error = None;
            let report = self.clients[i].tick(input, self.now, BUDGET, |bytes| {
                offer(
                    &mut self.link,
                    self.endpoints[i],
                    self.authority,
                    bytes,
                    self.now,
                    &mut error,
                )
            })?;
            fault(error)?;
            self.observe(i, report);
        }
        self.pump()?;
        let mut error = None;
        let output = self.server.tick(&[], self.now, BUDGET, |to, bytes| {
            let Some(i) = route(to) else {
                return false;
            };
            offer(
                &mut self.link,
                self.authority,
                self.endpoints[i],
                bytes,
                self.now,
                &mut error,
            )
        })?;
        fault(error)?;
        self.totals.refused += output.report.datagrams_refused;
        self.totals.accepted_events += output.events.len();
        self.pump()?;
        Ok(output)
    }
    fn drain(&mut self, report: &mut FrameReport) {
        for i in 0..2 {
            let events = self.clients[i].drain_events();
            self.totals.delivered_events[i] += events.len();
            report.delivered[i].extend(events);
        }
    }
    fn frame(
        &mut self,
        elapsed_us: u64,
        inputs: [InputFrame; 2],
        menu: bool,
    ) -> Result<FrameReport> {
        // Reject excessive host time instead of silently dropping authoritative
        // ticks. The bounded recipe uses <=46875us, including explicit zero frames.
        if elapsed_us > TICK_US * u64::from(MAX_FRAME_TICKS) {
            return Err("frame exceeds four-tick budget".into());
        }
        let frame_end = self
            .now
            .checked_add(Duration::from_micros(elapsed_us))
            .filter(|time| *time <= Duration::from_micros(1 << 53))
            .ok_or("host time exhausted")?;
        let first_tick_offset = TICK_US - self.accumulator_us;
        let accumulated = self.accumulator_us + elapsed_us;
        let issued = (accumulated / TICK_US) as u32;
        self.accumulator_us = accumulated % TICK_US;
        let mut report = FrameReport {
            issued,
            ..Default::default()
        };
        self.poll()?;
        self.pump()?;
        for index in 0..issued {
            self.now += Duration::from_micros(if index == 0 {
                first_tick_offset
            } else {
                TICK_US
            });
            self.poll()?;
            self.pump()?;
            // A local overlay masks controls, preserving each player's aim.
            // An interactive host would consume latched edges here, once for
            // this executed tick; this inspector supplies held scripted levels.
            let masked = std::array::from_fn(|i| {
                let local = self.clients[i].local_player();
                InputFrame {
                    yaw: local.map_or(0., |p| p.yaw),
                    pitch: local.map_or(0., |p| p.pitch),
                    ..Default::default()
                }
            });
            let output = self.tick(if menu { masked } else { inputs })?;
            report.executed += u32::from(output.report.tick_executed);
            report
                .accepted
                .extend(output.events.into_iter().map(|event| NetworkEvent {
                    tick: self.server.game.tick,
                    event,
                }));
            self.drain(&mut report);
        }
        self.drain(&mut report);
        self.now = frame_end;
        self.poll()?;
        self.pump()?; // Drained reliable tails progress without ingress.
        self.drain(&mut report);
        self.totals.frames += 1;
        self.totals.zero_tick_frames += usize::from(issued == 0);
        self.totals.issued_ticks += u64::from(report.issued);
        self.totals.executed_ticks += u64::from(report.executed);
        Ok(report)
    }
    fn status(&self) -> Result<()> {
        println!(
            "authority tick={} peers={} pending reliable={} queue={}/{} packets, {}/{} bytes",
            self.server.game.tick,
            self.server.peer_count(),
            self.server.pending_event_count(),
            self.link.queued_packets(),
            self.link.limits().packets,
            self.link.queued_bytes(),
            self.link.limits().bytes
        );
        for (i, client) in self.clients.iter().enumerate() {
            println!(
                "client {}: connected={} player={:?} snapshot={} ack={:?} pending inputs={} RTT={:?}",
                i + 1,
                client.is_connected(),
                client.player_id(),
                client.server_tick(),
                client.acknowledged_sequence(),
                client.pending_input_count(),
                client.round_trip_seconds()
            );
            if let Some(local) = client.local_player() {
                println!(
                    "  predicted local position={:?} health={} ammo={}",
                    local.position, local.health, local.ammo
                );
            }
            for player in client.rendered_players(self.now)? {
                println!(
                    "  rendered player={} position={:?} health={}",
                    player.id, player.position, player.health
                );
            }
        }
        println!("work: {:?}", self.totals);
        Ok(())
    }
}
fn impaired() -> Impairment {
    Impairment {
        delay: Duration::from_millis(8),
        loss_every: Some(11),
        reorder_every: Some(5),
        reorder_delay: Duration::from_millis(35),
    }
}
fn main() -> Result<()> {
    let mut ticks = 256u32;
    let mut impairment = Impairment::default();
    let mut reconnect_at = None;
    let mut limits = Limits::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ticks" => ticks = args.next().ok_or("--ticks needs a number")?.parse()?,
            "--queue-packets" => {
                limits.packets = args
                    .next()
                    .ok_or("--queue-packets needs a number")?
                    .parse()?
            }
            "--queue-bytes" => {
                limits.bytes = args.next().ok_or("--queue-bytes needs a number")?.parse()?
            }
            "--impaired" => impairment = impaired(),
            "--reconnect-at" => {
                reconnect_at = Some(
                    args.next()
                        .ok_or("--reconnect-at needs a tick")?
                        .parse::<u32>()?,
                )
            }
            "--help" => {
                println!(
                    "local_session [--ticks 1..8192] [--impaired] [--reconnect-at TICK]\n\
                    Two independent production clients, one authority, bounded production local link.\n\
                    Scripted movement/fire/reload in the authored arena; local menu masks inputs\n\
                    in frames starting at ticks 96..128; authority/network keep progressing.\n\
                    Deterministic supplied time; 64Hz ticks, zero-tick and three-tick frames;\n\
                    <=4 ticks/frame at exact boundaries, <=14 pumps/frame,\n\
                    <=192 ingress/pump, 8 output datagrams/core call,\n\
                    128 packets/153600 bytes/router. Impairment: 8ms delay, every 11th\n\
                    admission lost, every 5th delayed another 35ms. Reports only accepted\n\
                    authority/client events and actual predicted/rendered state. No sockets/sleep.\n\
                    Optional --queue-packets 0..4096 and --queue-bytes 0..4915200 set\n\
                    aggregate capacity (zero refuses all); insufficient capacity may prevent joining.\n\
                    Ends with 128 shutdown maintenance frames (one supplied second) to drain.\n\
                    Headless network inspection is not a browser/physical-device/audio playtest."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if !(1..=8192).contains(&ticks) {
        return Err("--ticks must be 1..8192".into());
    }
    let mut session = LocalSession::new(1, Arena::default(), impairment, limits)?;
    println!("Headless scripted local session: 64Hz, target={ticks}, impairment={impairment:?}");
    let deltas = [0, 7812, 7813, 46875];
    for frame in 0..(ticks as usize * 4 + 4) {
        if session.server.game.tick >= ticks {
            break;
        }
        if reconnect_at.is_some_and(|at| session.server.game.tick >= at) {
            session.reconnect(0)?;
            reconnect_at = None;
            println!("fresh-nonce client 1 reconnect");
        }
        let tick = session.server.game.tick;
        let input = InputFrame {
            move_x: if (tick / 64).is_multiple_of(2) {
                0.3
            } else {
                -0.3
            },
            buttons: if tick.is_multiple_of(32) {
                BUTTON_FIRE
            } else if tick.is_multiple_of(121) {
                BUTTON_RELOAD
            } else {
                0
            },
            ..Default::default()
        };
        // Clamp final batch to requested ticks without losing already accumulated
        // fractional time. Inputs here are scripted held levels, not input edges.
        let remaining_us = u64::from(ticks - tick) * TICK_US - session.accumulator_us;
        let report = session.frame(
            deltas[frame % 4].min(remaining_us),
            [input, InputFrame::default()],
            (96..128).contains(&tick),
        )?;
        for accepted in report.accepted {
            println!(
                "authority accepted tick={}: {:?}",
                accepted.tick, accepted.event
            );
        }
        for (i, events) in report.delivered.iter().enumerate() {
            for event in events {
                println!(
                    "client {} accepted event tick={}: {:?}",
                    i + 1,
                    event.tick,
                    event.event
                );
            }
        }
        if tick / 64 != session.server.game.tick / 64 {
            session.status()?;
        }
    }
    // Shutdown drain: stop issuing game ticks, but keep protocol retries live.
    // Local menu frames above continue the ordinary fixed tick schedule.
    for frame in 0..128 {
        session.now += Duration::from_micros(if frame % 2 == 0 { 7812 } else { 7813 });
        let report = session.frame(0, [InputFrame::default(); 2], false)?;
        for (i, events) in report.delivered.iter().enumerate() {
            for event in events {
                println!(
                    "client {} accepted event tick={}: {:?}",
                    i + 1,
                    event.tick,
                    event.event
                );
            }
        }
    }
    session.status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_fps::GameEvent;
    use ferravex_math::Vec3;
    use ferravex_net::{ClientExtension, ClientPacket, decode_client};
    fn range() -> Arena {
        Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.)],
        }
    }
    fn session(id: u64, impairment: Impairment) -> LocalSession {
        LocalSession::new(id, range(), impairment, Limits::default()).unwrap()
    }
    fn join(s: &mut LocalSession) {
        for _ in 0..128 {
            s.frame(7813, [InputFrame::default(); 2], false).unwrap();
            if s.clients.iter().all(ClientState::is_connected)
                && s.clients.iter().all(|c| c.retained_baseline_count() > 0)
            {
                return;
            }
        }
        panic!("bounded handshake did not finish");
    }
    fn drain_shutdown(s: &mut LocalSession, delivered: &mut [Vec<NetworkEvent>; 2]) {
        // Supplied-time shutdown maintenance deliberately stops issuing new game
        // ticks, allowing retransmissions to drain. This is not a local menu.
        for _ in 0..256 {
            s.now += Duration::from_micros(7813);
            let r = s.frame(0, [InputFrame::default(); 2], false).unwrap();
            assert_eq!((r.issued, r.executed), (0, 0));
            for (i, events) in r.delivered.into_iter().enumerate() {
                delivered[i].extend(events);
            }
        }
        // Stop initiating periodic probes, then deliver their finite in-flight
        // responses/ACKs. Otherwise the final poll may legitimately leave a new
        // delayed Ping in the link even after every gameplay event was ACKed.
        let impairment = s.link.impairment();
        for _ in 0..8 {
            s.now += impairment.delay + impairment.reorder_delay;
            s.pump().unwrap();
            let mut report = FrameReport::default();
            s.drain(&mut report);
            for (i, events) in report.delivered.into_iter().enumerate() {
                delivered[i].extend(events);
            }
        }
    }
    #[test]
    fn two_independent_live_sessions_handshake_prediction_ack_reconciliation_and_rendering() {
        let mut a = session(101, Impairment::default());
        let mut b = session(102, Impairment::default());
        join(&mut a);
        join(&mut b);
        assert_eq!(a.server.peer_count(), 2);
        assert_eq!(b.server.peer_count(), 2);
        assert_eq!(a.clients[0].player_id(), Some(0));
        assert_eq!(a.clients[1].player_id(), Some(1));
        assert_eq!(
            a.link.send(b.endpoints[0], a.authority, b"foreign", a.now),
            Err(LinkError::StaleEndpoint)
        );
        let initial = a.clients[0].local_player().unwrap().position;
        for n in 0..96 {
            let dt = [0, 7812, 7813, 46875][n % 4];
            let r = a
                .frame(
                    dt,
                    [
                        InputFrame {
                            move_x: 1.,
                            ..Default::default()
                        },
                        InputFrame::default(),
                    ],
                    false,
                )
                .unwrap();
            let q = b
                .frame(
                    dt,
                    [
                        InputFrame {
                            move_x: -1.,
                            ..Default::default()
                        },
                        InputFrame::default(),
                    ],
                    false,
                )
                .unwrap();
            assert_eq!(r.issued, r.executed);
            assert_eq!(q.issued, q.executed);
            assert!(a.link.queued_packets() <= a.link.limits().packets);
            assert!(a.link.queued_bytes() <= a.link.limits().bytes);
        }
        assert_eq!(a.totals.issued_ticks, a.totals.executed_ticks);
        assert_eq!(
            a.totals.prediction_ticks,
            [a.totals.executed_ticks as usize; 2]
        );
        assert!(a.totals.zero_tick_frames > 0);
        assert!(a.clients[0].local_player().unwrap().position.x > initial.x);
        assert!(b.clients[0].local_player().unwrap().position.x < initial.x);
        for s in [&a, &b] {
            assert!(s.totals.snapshots > 2);
            assert!(s.totals.acknowledged_inputs > 0);
            for c in &s.clients {
                assert!(c.acknowledged_sequence().is_some());
                assert!(c.pending_input_count() <= 128);
                assert!(c.retained_baseline_count() <= 32);
                assert_eq!(c.rendered_players(s.now).unwrap().len(), 2);
                assert!(c.round_trip_seconds().is_some());
            }
        }
    }
    #[test]
    fn impaired_production_combat_delivers_exact_accepted_events_once_and_recovers() {
        let run = || {
            let mut s = session(
                201,
                Impairment {
                    delay: Duration::from_millis(2),
                    loss_every: Some(7),
                    reorder_every: Some(3),
                    reorder_delay: Duration::from_millis(15),
                },
            );
            join(&mut s);
            let mut accepted = Vec::new();
            let mut delivered = [Vec::new(), Vec::new()];
            for n in 0..160 {
                let r = s
                    .frame(
                        TICK_US,
                        [
                            InputFrame {
                                buttons: if n < 96 { BUTTON_FIRE } else { 0 },
                                ..Default::default()
                            },
                            InputFrame::default(),
                        ],
                        false,
                    )
                    .unwrap();
                accepted.extend(r.accepted);
                for (i, events) in r.delivered.into_iter().enumerate() {
                    delivered[i].extend(events);
                }
            }
            drain_shutdown(&mut s, &mut delivered);
            assert!(
                accepted
                    .iter()
                    .any(|e| matches!(e.event, GameEvent::Shot { .. }))
            );
            assert!(
                accepted
                    .iter()
                    .any(|e| matches!(e.event, GameEvent::Hit { .. }))
            );
            for events in &delivered {
                assert_eq!(events, &accepted);
            }
            assert_eq!(s.server.pending_event_count(), 0);
            assert_eq!(s.link.queued_packets(), 0);
            assert!(s.totals.lost > 0);
            assert!(s.totals.replayed_inputs > 0);
            assert!(s.clients.iter().all(ClientState::is_connected));
            (accepted, s.totals)
        };
        assert_eq!(run(), run());
    }
    #[test]
    fn full_queue_refusal_keeps_real_reliable_head_due_at_unchanged_time() {
        for byte_capacity in [false, true] {
            let limits = if byte_capacity {
                Limits {
                    packets: 128,
                    bytes: 1024,
                }
            } else {
                Limits::default()
            };
            let mut s = LocalSession::new(301, range(), Impairment::default(), limits).unwrap();
            join(&mut s);
            let report = s.clients[0]
                .tick(
                    InputFrame {
                        buttons: BUTTON_FIRE,
                        ..Default::default()
                    },
                    s.now,
                    BUDGET,
                    |bytes| {
                        s.link
                            .send(s.endpoints[0], s.authority, bytes, s.now)
                            .is_ok()
                    },
                )
                .unwrap();
            assert!(report.tick_executed);
            s.pump().unwrap();
            // Capture one actual complete production input datagram. Repeating it
            // below is explicitly hostile duplicate saturation traffic, not an ACK
            // or an invented authority/game outcome.
            let mut input_wire = Vec::new();
            s.clients[1]
                .tick(InputFrame::default(), s.now, BUDGET, |bytes| {
                    input_wire = bytes.to_vec();
                    true
                })
                .unwrap();
            assert!(matches!(
                decode_client(&input_wire),
                Ok(ClientPacket::Extension(ClientExtension::Inputs { .. }))
            ));
            // Byte saturation uses explicitly malformed 64-byte ingress records;
            // packet saturation uses captured duplicate input datagrams. Neither
            // kind invents protocol acknowledgements or gameplay outcomes.
            let padding = [0xAA; 64];
            let fill = if byte_capacity {
                padding.as_slice()
            } else {
                &input_wire
            };
            let fill_count = if byte_capacity { 16 } else { limits.packets };
            for _ in 0..fill_count {
                s.link
                    .send(s.endpoints[1], s.authority, fill, s.now)
                    .unwrap();
            }
            assert_eq!(
                s.link.send(s.endpoints[1], s.authority, b"full", s.now),
                Err(if byte_capacity {
                    LinkError::ByteCapacity
                } else {
                    LinkError::PacketCapacity
                })
            );
            let out = s
                .server
                .tick(&[], s.now, BUDGET, |to, bytes| {
                    s.link
                        .send(s.authority, s.endpoints[route(to).unwrap()], bytes, s.now)
                        .is_ok()
                })
                .unwrap();
            assert!(out.report.tick_executed);
            assert!(!out.events.is_empty());
            assert!(out.report.datagrams_refused > 0);
            assert!(out.report.reliable_remaining_due > 0);
            let tick = s.server.game.tick;
            // Discard the explicitly labelled duplicate saturation traffic, not
            // fabricated ACKs. Original refused events remain in the driver.
            let mut bytes = [0; MAX_PACKET_BYTES];
            for _ in 0..fill_count {
                assert!(
                    s.link
                        .receive(s.authority, s.now, &mut bytes)
                        .unwrap()
                        .is_some()
                );
            }
            s.poll().unwrap(); // Same supplied timestamp: reliable retry must be due.
            s.pump().unwrap();
            for client in &mut s.clients {
                assert_eq!(
                    client
                        .drain_events()
                        .iter()
                        .map(|e| e.event)
                        .collect::<Vec<_>>(),
                    out.events
                );
            }
            s.poll().unwrap();
            s.pump().unwrap();
            assert_eq!(s.server.game.tick, tick);
            assert_eq!(s.server.pending_event_count(), 0);
        }
    }
    #[test]
    fn refused_hello_and_input_retry_same_time_and_malformed_datagrams_are_driver_rejections() {
        let mut s = session(401, Impairment::default());
        // Opaque malformed datagrams exercise local byte transport, not gameplay.
        for _ in 0..128 {
            s.link
                .send(s.endpoints[1], s.authority, b"bad", s.now)
                .unwrap();
        }
        s.poll().unwrap();
        assert!(!s.clients[0].is_connected());
        assert!(s.totals.refused >= 2);
        s.pump().unwrap();
        assert_eq!(s.server.rejected_packets, 64); // 4 passes * 16 authority receives.
        s.pump().unwrap();
        assert_eq!(s.server.rejected_packets, 128);
        s.poll().unwrap();
        s.pump().unwrap();
        assert!(s.clients.iter().all(ClientState::is_connected));
        let mut bytes = [0; MAX_PACKET_BYTES];
        s.poll().unwrap();
        s.pump().unwrap();
        for _ in 0..128 {
            s.link
                .send(s.endpoints[1], s.authority, b"bad", s.now)
                .unwrap();
        }
        let before = s.clients[0].local_player().unwrap().position;
        let r = s.clients[0]
            .tick(
                InputFrame {
                    move_x: 1.,
                    ..Default::default()
                },
                s.now,
                BUDGET,
                |b| s.link.send(s.endpoints[0], s.authority, b, s.now).is_ok(),
            )
            .unwrap();
        assert!(r.tick_executed);
        assert_eq!(r.datagrams_refused, 1);
        let predicted = s.clients[0].local_player().unwrap().position;
        assert!(predicted.x > before.x);
        for _ in 0..128 {
            s.link
                .receive(s.authority, s.now, &mut bytes)
                .unwrap()
                .unwrap();
        }
        s.poll().unwrap();
        let mut found = false;
        for _ in 0..128 {
            let Some(d) = s.link.receive(s.authority, s.now, &mut bytes).unwrap() else {
                break;
            };
            if let Ok(ClientPacket::Extension(ClientExtension::Inputs { inputs, .. })) =
                decode_client(&bytes[..d.len])
            {
                assert_eq!(inputs.last().unwrap().input.sequence, 0);
                found = true;
            }
        }
        assert!(found);
        assert_eq!(s.clients[0].local_player().unwrap().position, predicted);
        assert_eq!(s.clients[0].pending_input_count(), 1); // Admission is not ACK.
    }
    #[test]
    fn catchup_inputs_use_exact_tick_boundaries_and_host_time_errors_preflight() {
        let mut s = session(
            601,
            Impairment {
                delay: Duration::from_millis(100),
                ..Default::default()
            },
        );
        join(&mut s);
        let start = s.now;
        let first = start + Duration::from_micros(TICK_US - s.accumulator_us);
        let r = s
            .frame(3 * TICK_US, [InputFrame::default(); 2], false)
            .unwrap();
        assert_eq!((r.issued, r.executed), (3, 3));
        assert_eq!(s.now, start + Duration::from_micros(3 * TICK_US));
        // Inspect original encoded inputs admitted by the actual tick calls.
        // This deliberately ends the fixture without feeding these packets on.
        let mut bytes = [0; MAX_PACKET_BYTES];
        let mut times = Vec::new();
        for _ in 0..s.link.limits().packets {
            let Some(d) = s
                .link
                .receive(s.authority, s.now + Duration::from_secs(1), &mut bytes)
                .unwrap()
            else {
                break;
            };
            if d.from == s.endpoints[0]
                && d.admitted_at > start
                && matches!(
                    decode_client(&bytes[..d.len]),
                    Ok(ClientPacket::Extension(ClientExtension::Inputs { .. }))
                )
            {
                times.push(d.admitted_at);
            }
        }
        assert_eq!(
            times,
            [
                first,
                first + Duration::from_micros(TICK_US),
                first + Duration::from_micros(2 * TICK_US)
            ]
        );

        let mut s = session(602, Impairment::default());
        // Host time-domain boundary setup, not a simulation/outcome mutation.
        s.now = Duration::from_micros(1 << 53);
        let tick = s.server.game.tick;
        assert!(s.frame(1, [InputFrame::default(); 2], false).is_err());
        assert_eq!(s.now, Duration::from_micros(1 << 53));
        assert_eq!(s.accumulator_us, 0);
        assert_eq!(s.server.game.tick, tick);
        assert_eq!(s.link.queued_packets(), 0);
        assert_eq!(s.totals, Totals::default());
    }

    #[test]
    fn local_menu_zero_and_catchup_frames_keep_authority_and_polling_live_reset_is_fresh() {
        let mut s = session(501, Impairment::default());
        join(&mut s);
        let before = s.clients[0].local_player().unwrap().clone();
        for _ in 0..4 {
            let r = s
                .frame(
                    0,
                    [InputFrame {
                        buttons: BUTTON_FIRE,
                        move_x: 1.,
                        ..Default::default()
                    }; 2],
                    true,
                )
                .unwrap();
            assert_eq!((r.issued, r.executed), (0, 0));
        }
        assert_eq!(s.clients[0].local_player(), Some(&before));
        let tick = s.server.game.tick;
        for _ in 0..8 {
            let r = s
                .frame(
                    3 * TICK_US,
                    [InputFrame {
                        buttons: BUTTON_FIRE,
                        move_x: 1.,
                        ..Default::default()
                    }; 2],
                    true,
                )
                .unwrap();
            assert_eq!((r.issued, r.executed), (3, 3));
            assert!(r.accepted.is_empty());
        }
        assert_eq!(s.server.game.tick, tick + 24);
        assert_eq!(
            s.clients[0].local_player().unwrap().position,
            before.position
        );
        assert!(s.clients[0].acknowledged_sequence().is_some());
        let old = s.endpoints[0];
        let authority = s.server.game.player(0).unwrap().clone();
        s.reconnect(0).unwrap();
        assert_eq!(s.clients[0].pending_input_count(), 0);
        assert_eq!(s.clients[0].retained_baseline_count(), 0);
        assert_eq!(
            s.link.send(old, s.authority, b"old", s.now),
            Err(LinkError::StaleEndpoint)
        );
        join(&mut s); // Hello rate gate requires later supplied time, not polling loops.
        assert_eq!(
            s.server.game.player(0).unwrap().position,
            authority.position
        );
        assert_eq!(s.server.game.player(0).unwrap().health, authority.health);
        assert_ne!(old, s.endpoints[0]);
        assert_eq!(s.server.peer_count(), 2);
        let now = s.now;
        let tick = s.server.game.tick;
        assert!(
            s.frame(4 * TICK_US + 1, [InputFrame::default(); 2], false)
                .is_err()
        );
        assert_eq!(s.now, now);
        assert_eq!(s.server.game.tick, tick);
    }
}
