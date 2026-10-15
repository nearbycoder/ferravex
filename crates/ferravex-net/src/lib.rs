//! Bounded, versioned network protocol shared by native and browser game clients.
//!
//! The protocol never accepts client positions or client-selected time steps.
//! Native UDP is intentionally a thin primitive: authentication, encryption,
//! browser transports, and NAT traversal are outside this prototype.
pub mod delta;
pub mod history;
pub mod reliable;

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
/// Version 3 carries jump grace/buffer timers and held state for exact motor replay.
pub const PROTOCOL_VERSION: u8 = 3;

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

/// Deduplicates input redundancy and bounds per-client simulation backlog.
#[derive(Debug, Default)]
pub struct InputQueue {
    queue: VecDeque<InputFrame>,
    newest: Option<u32>,
}
impl InputQueue {
    pub fn push(&mut self, input: InputFrame) -> bool {
        if !input.is_valid()
            || self.queue.len() >= MAX_QUEUED_INPUTS
            || self
                .newest
                .is_some_and(|old| !sequence_newer(input.sequence, old))
        {
            return false;
        }
        self.newest = Some(input.sequence);
        self.queue.push_back(input);
        true
    }
    pub fn pop(&mut self) -> Option<InputFrame> {
        self.queue.pop_front()
    }
    /// Coalesce a burst to its newest movement/look sample without losing a
    /// queued action press. The authority still simulates exactly one fixed step.
    /// This prevents redundant commands recovered after loss from adding permanent
    /// control latency. Previously consumed commands cannot re-enter the queue.
    pub fn pop_latest(&mut self) -> Option<InputFrame> {
        let mut latest = self.queue.pop_back()?;
        for older in self.queue.drain(..) {
            latest.buttons |= older.buttons;
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

fn header(kind: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAX_PACKET_BYTES);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&[PROTOCOL_VERSION, kind]);
    out
}
fn put_u32(out: &mut Vec<u8>, n: u32) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, n: u64) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_f32(out: &mut Vec<u8>, n: f32) {
    put_u32(out, n.to_bits());
}
fn put_u16(out: &mut Vec<u8>, n: u16) {
    out.extend_from_slice(&n.to_le_bytes());
}
fn put_input(out: &mut Vec<u8>, input: &InputFrame) {
    put_u32(out, input.sequence);
    for n in [input.move_x, input.move_z, input.yaw, input.pitch] {
        put_f32(out, n);
    }
    out.push(input.buttons);
}

pub fn encode_client(packet: &ClientPacket) -> Result<Vec<u8>, ProtocolError> {
    let mut out = match packet {
        ClientPacket::EventAck { .. } => header(6),
        ClientPacket::Hello { .. } => header(1),
        ClientPacket::Input { .. } => header(2),
    };
    match packet {
        ClientPacket::EventAck { session, sequence } => {
            if *session == 0 {
                return Err(ProtocolError::Invalid);
            }
            put_u64(&mut out, *session);
            put_u64(&mut out, *sequence);
        }
        ClientPacket::Hello { nonce } => put_u64(&mut out, *nonce),
        ClientPacket::Input { session, inputs } => {
            if *session == 0
                || inputs.is_empty()
                || inputs.len() > MAX_INPUTS_PER_PACKET
                || inputs.iter().any(|input| !input.is_valid())
            {
                return Err(ProtocolError::Invalid);
            }
            put_u64(&mut out, *session);
            out.push(inputs.len() as u8);
            for input in inputs {
                put_input(&mut out, input);
            }
        }
    }
    Ok(out)
}
pub fn encode_server(packet: &ServerPacket) -> Result<Vec<u8>, ProtocolError> {
    let mut out = match packet {
        ServerPacket::ReliableEvent { .. } => header(5),
        ServerPacket::Welcome { .. } => header(3),
        ServerPacket::Snapshot(_) => header(4),
    };
    match packet {
        ServerPacket::ReliableEvent { session, event } => {
            if *session == 0 || event.payload.len() > reliable::EVENT_BYTES {
                return Err(ProtocolError::Invalid);
            }
            put_u64(&mut out, *session);
            put_u64(&mut out, event.sequence);
            put_u16(&mut out, event.payload.len() as u16);
            out.extend_from_slice(&event.payload);
        }
        ServerPacket::Welcome {
            nonce,
            session,
            player_id,
            tick,
        } => {
            if *session == 0 || *player_id as usize >= MAX_PLAYERS {
                return Err(ProtocolError::Invalid);
            }
            put_u64(&mut out, *nonce);
            put_u64(&mut out, *session);
            out.push(*player_id);
            put_u32(&mut out, *tick);
        }
        ServerPacket::Snapshot(snapshot) => {
            if snapshot.session == 0 || snapshot.players.len() > MAX_PLAYERS {
                return Err(ProtocolError::Invalid);
            }
            let mut ids = 0u16;
            for p in &snapshot.players {
                if !p.is_valid() || ids & (1u16 << p.id) != 0 {
                    return Err(ProtocolError::Invalid);
                }
                ids |= 1u16 << p.id;
            }
            put_u64(&mut out, snapshot.session);
            put_u32(&mut out, snapshot.tick);
            put_u32(&mut out, snapshot.ack_sequence);
            out.push(snapshot.ack_valid as u8);
            out.push(snapshot.players.len() as u8);
            for p in &snapshot.players {
                out.push(p.id);
                for n in p
                    .position
                    .into_iter()
                    .chain(p.velocity)
                    .chain([p.yaw, p.pitch])
                {
                    put_f32(&mut out, n);
                }
                out.extend_from_slice(&[p.health, p.ammo, p.grounded as u8]);
                put_f32(&mut out, p.jump_coyote);
                put_f32(&mut out, p.jump_buffer);
                out.push(p.jump_held as u8);
                put_u16(&mut out, p.fire_cooldown);
                put_u16(&mut out, p.reload_ticks);
                put_u16(&mut out, p.respawn_ticks);
            }
        }
    }
    if out.len() > MAX_PACKET_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    Ok(out)
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
            ServerPacket::Snapshot(Snapshot {
                session,
                tick,
                ack_sequence,
                ack_valid,
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
            session: 99,
            tick: 44,
            ack_valid: true,
            ack_sequence: 3,
            players,
        });
        let bytes = encode_server(&packet).unwrap();
        assert_eq!(bytes.len(), 840);
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
