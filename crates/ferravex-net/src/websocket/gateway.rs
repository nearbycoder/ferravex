use super::*;
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::{Duration, Instant},
};
const MAX_CONNECTIONS: usize = 16;
const MAX_PENDING: usize = 64 * 1024;
#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub authority: SocketAddr,
    pub allowed_origins: Vec<String>,
}
impl Config {
    pub fn new(bind: SocketAddr, mut authority: SocketAddr) -> Self {
        if authority.ip().is_unspecified() {
            authority.set_ip(if authority.is_ipv4() {
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            } else {
                IpAddr::V6(Ipv6Addr::LOCALHOST)
            });
        }
        Self {
            bind,
            authority,
            allowed_origins: vec![
                "http://127.0.0.1:8087".into(),
                "http://localhost:8087".into(),
                "http://[::1]:8087".into(),
            ],
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub accepted: u64,
    pub closed: u64,
    pub rejected: u64,
    pub io_errors: u64,
    pub client_bytes: u64,
    pub authority_bytes: u64,
}
/// Nonblocking first-party WebSocket → UDP bridge. Each browser gets a distinct
/// UDP endpoint/session at the existing authority. Call poll before/after its
/// fixed tick. The bridge never computes movement, accepts hits or invents ACKs.
pub struct Gateway {
    listener: TcpListener,
    config: Config,
    peers: Vec<Peer>,
    stats: Stats,
}
impl Gateway {
    pub fn bind(config: Config) -> io::Result<Self> {
        if config.authority.port() == 0
            || config.allowed_origins.len() > 32
            || config
                .allowed_origins
                .iter()
                .any(|s| s.len() > 512 || s.bytes().any(|b| b < 32 || b == 127))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid gateway configuration",
            ));
        }
        let listener = TcpListener::bind(config.bind)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            config,
            peers: Vec::with_capacity(MAX_CONNECTIONS),
            stats: Stats::default(),
        })
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    pub fn connection_count(&self) -> usize {
        self.peers.len()
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn poll(&mut self) -> io::Result<()> {
        for _ in 0..4 {
            let (stream, _) = match self.listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            };
            if self.peers.len() == MAX_CONNECTIONS {
                self.stats.rejected += 1;
                continue;
            }
            match Peer::new(stream, self.config.authority) {
                Ok(peer) => {
                    self.peers.push(peer);
                    self.stats.accepted += 1;
                }
                Err(_) => self.stats.io_errors += 1,
            }
        }
        let origins = &self.config.allowed_origins;
        let stats = &mut self.stats;
        self.peers
            .retain_mut(|peer| match peer.poll(origins, stats) {
                Ok(true) => true,
                Ok(false) => {
                    stats.closed += 1;
                    false
                }
                Err(_) => {
                    stats.closed += 1;
                    stats.io_errors += 1;
                    false
                }
            });
        Ok(())
    }
}
struct Peer {
    tcp: TcpStream,
    udp: UdpSocket,
    decoder: Decoder,
    ready: bool,
    created: Instant,
    last: Instant,
    closing: Option<Instant>,
    out: VecDeque<Vec<u8>>,
    offset: usize,
    pending: usize,
}
impl Peer {
    fn new(tcp: TcpStream, authority: SocketAddr) -> io::Result<Self> {
        tcp.set_nonblocking(true)?;
        tcp.set_nodelay(true)?;
        let udp = UdpSocket::bind(if authority.is_ipv4() {
            SocketAddr::from(([0, 0, 0, 0], 0))
        } else {
            SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))
        })?;
        udp.connect(authority)?;
        udp.set_nonblocking(true)?;
        let now = Instant::now();
        Ok(Self {
            tcp,
            udp,
            decoder: Decoder::default(),
            ready: false,
            created: now,
            last: now,
            closing: None,
            out: VecDeque::new(),
            offset: 0,
            pending: 0,
        })
    }
    fn queue(&mut self, bytes: Vec<u8>) -> io::Result<()> {
        if bytes.len() > MAX_PENDING - self.pending || self.out.len() == 256 {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "WebSocket backpressure limit",
            ));
        }
        self.pending += bytes.len();
        self.out.push_back(bytes);
        Ok(())
    }
    fn close(&mut self, code: u16) -> io::Result<()> {
        self.out.clear();
        self.pending = 0;
        self.offset = 0;
        self.closing = Some(Instant::now());
        self.queue(frame(8, &code.to_be_bytes()))
    }
    fn flush(&mut self) -> io::Result<()> {
        for _ in 0..8 {
            let Some(bytes) = self.out.front() else { break };
            match self.tcp.write(&bytes[self.offset..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                Ok(n) => {
                    self.offset += n;
                    self.pending -= n;
                    if self.offset == bytes.len() {
                        self.out.pop_front();
                        self.offset = 0;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn poll(&mut self, origins: &[String], stats: &mut Stats) -> io::Result<bool> {
        if self.closing.is_none() {
            let mut buffer = [0u8; 4096];
            match self.tcp.read(&mut buffer) {
                Ok(0) => return Ok(false),
                Ok(n) => {
                    if let Err(code) = self.decoder.append(&buffer[..n]) {
                        stats.rejected += 1;
                        if self.ready {
                            self.close(code)?;
                        } else {
                            return Ok(false);
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
            if !self.ready && self.closing.is_none() {
                if self.created.elapsed() > Duration::from_secs(5) {
                    return Ok(false);
                }
                match handshake(&self.decoder.bytes, origins) {
                    Ok(Some((used, response))) => {
                        self.decoder.bytes.drain(..used);
                        self.queue(response)?;
                        self.ready = true;
                        self.last = Instant::now();
                    }
                    Ok(None) => {}
                    Err(code) => {
                        stats.rejected += 1;
                        self.closing = Some(Instant::now());
                        self.queue(format!("HTTP/1.1 {code} Rejected\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").into_bytes())?;
                    }
                }
            }
            if self.ready && self.closing.is_none() {
                for _ in 0..16 {
                    match self.decoder.next() {
                        Ok(None) => break,
                        Ok(Some(message)) => {
                            self.last = Instant::now();
                            match message {
                                Message::Binary(bytes) => match self.udp.send(&bytes) {
                                    Ok(n) => stats.client_bytes += n as u64,
                                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                                    Err(e) => return Err(e),
                                },
                                Message::Ping(bytes) => self.queue(frame(10, &bytes))?,
                                Message::Pong => {}
                                Message::Close(bytes) => {
                                    self.out.clear();
                                    self.offset = 0;
                                    self.pending = 0;
                                    self.closing = Some(Instant::now());
                                    self.queue(frame(8, &bytes))?;
                                    break;
                                }
                            }
                        }
                        Err(code) => {
                            stats.rejected += 1;
                            self.close(code)?;
                            break;
                        }
                    }
                }
                if self.last.elapsed() > Duration::from_secs(10) {
                    self.close(1001)?;
                }
                if self.closing.is_none() {
                    let mut data = [0u8; MAX_PACKET_BYTES + 1];
                    for _ in 0..16 {
                        match self.udp.recv(&mut data) {
                            Ok(n) if n <= MAX_PACKET_BYTES => {
                                self.queue(frame(2, &data[..n]))?;
                                stats.authority_bytes += n as u64;
                            }
                            Ok(_) => stats.rejected += 1,
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        }
        self.flush()?;
        Ok(self
            .closing
            .is_none_or(|started| self.pending != 0 && started.elapsed() < Duration::from_secs(1)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    // A single poll cannot guarantee that loopback TCP bytes have reached the
    // kernel receive queue, especially on a loaded macOS runner. Keep driving
    // the nonblocking gateway while awaiting each real transport milestone.
    fn pump_until<T>(
        gateway: &mut Gateway,
        mut receive: impl FnMut() -> io::Result<Option<T>>,
    ) -> T {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            gateway.poll().unwrap();
            match receive() {
                Ok(Some(value)) => return value,
                Ok(None) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("loopback I/O failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "gateway loopback milestone timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn read_until(
        gateway: &mut Gateway,
        client: &mut TcpStream,
        complete: impl Fn(&[u8]) -> bool,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        pump_until(gateway, || {
            let mut part = [0; 4096];
            let count = client.read(&mut part)?;
            if count == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            bytes.extend_from_slice(&part[..count]);
            assert!(bytes.len() <= 4096, "unexpected loopback response size");
            Ok(complete(&bytes).then_some(()))
        });
        bytes
    }
    #[test]
    fn actual_tcp_upgrade_binary_udp_return_and_close() {
        let authority = UdpSocket::bind("127.0.0.1:0").unwrap();
        authority.set_nonblocking(true).unwrap();
        let mut gateway = Gateway::bind(Config::new(
            "127.0.0.1:0".parse().unwrap(),
            authority.local_addr().unwrap(),
        ))
        .unwrap();
        let mut client = TcpStream::connect(gateway.local_addr().unwrap()).unwrap();
        client.set_nonblocking(true).unwrap();
        client.write_all(b"GET /ferravex HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").unwrap();
        let header = read_until(&mut gateway, &mut client, |bytes| {
            bytes.ends_with(b"\r\n\r\n")
        });
        assert!(
            std::str::from_utf8(&header)
                .unwrap()
                .contains("101 Switching Protocols")
        );
        client
            .write_all(&super::super::tests::masked(2, true, b"packet"))
            .unwrap();
        let mut buf = [0; 4096];
        let (n, source) = pump_until(&mut gateway, || authority.recv_from(&mut buf).map(Some));
        assert_eq!(&buf[..n], b"packet");
        authority.send_to(b"reply", source).unwrap();
        let reply = frame(2, b"reply");
        assert_eq!(
            read_until(&mut gateway, &mut client, |bytes| bytes.len()
                >= reply.len()),
            reply
        );
        client
            .write_all(&super::super::tests::masked(
                8,
                true,
                &1000u16.to_be_bytes(),
            ))
            .unwrap();
        let closed = frame(8, &1000u16.to_be_bytes());
        assert_eq!(
            read_until(&mut gateway, &mut client, |bytes| bytes.len()
                >= closed.len()),
            closed
        );
        assert_eq!(gateway.connection_count(), 0);
        assert_eq!(gateway.stats().client_bytes, 6);
    }
}
