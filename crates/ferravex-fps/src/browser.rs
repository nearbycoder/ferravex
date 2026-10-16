//! Browser datagram adapter for the same Rust authority/client session used by
//! native UDP. A first-party host supplies a bounded unreliable byte channel,
//! monotonic time and a fresh random seed. No browser/OS calls occur here.
use ferravex_net::MAX_PACKET_BYTES;
use std::{
    cell::RefCell,
    collections::VecDeque,
    io,
    net::{SocketAddr, ToSocketAddrs},
};
const QUEUE: usize = 128;
#[derive(Default)]
struct Bridge {
    incoming: VecDeque<Vec<u8>>,
    outgoing: VecDeque<Vec<u8>>,
    seed: u64,
    rejected: u64,
}
thread_local! {static BRIDGE:RefCell<Bridge>=RefCell::new(Bridge::default());}
pub fn reset(seed: u64) {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        b.incoming.clear();
        b.outgoing.clear();
        b.seed = seed.max(1);
        b.rejected = 0;
    });
}
pub fn deliver(bytes: &[u8]) -> bool {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        if bytes.is_empty() || bytes.len() > MAX_PACKET_BYTES {
            b.rejected = b.rejected.saturating_add(1);
            return false;
        }
        // Background tabs may render at 1 Hz. Reliable retries must not fill
        // the mailbox with identical datagrams before the game can ACK them.
        if b.incoming.iter().any(|p| p == bytes) {
            return true;
        }
        if b.incoming.len() == QUEUE {
            b.rejected = b.rejected.saturating_add(1);
            return false;
        }
        b.incoming.push_back(bytes.to_vec());
        true
    })
}
pub fn take_outgoing() -> Option<Vec<u8>> {
    BRIDGE.with(|b| b.borrow_mut().outgoing.pop_front())
}
pub fn pending() -> (usize, usize) {
    BRIDGE.with(|b| {
        let b = b.borrow();
        (b.incoming.len(), b.outgoing.len())
    })
}
pub fn rejected() -> u64 {
    BRIDGE.with(|b| b.borrow().rejected)
}
pub fn set_time(micros: u64) -> bool {
    ferravex_net::time::set_browser_time(micros)
}
pub(crate) fn nonce() -> u64 {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        b.seed = b.seed.wrapping_add(0x9e3779b97f4a7c15);
        ferravex_play::replay::derive_seed(b.seed, 0, 19).max(1)
    })
}
fn address(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}
pub(crate) struct Socket {
    local: SocketAddr,
    remote: Option<SocketAddr>,
}
impl Socket {
    pub fn bind(_addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self {
            local: address(1),
            remote: None,
        })
    }
    pub fn connect(_addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self {
            local: address(2),
            remote: Some(address(1)),
        })
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }
    pub fn send(&self, bytes: &[u8]) -> io::Result<usize> {
        self.send_to(
            bytes,
            self.remote.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "browser channel has no authority",
                )
            })?,
        )
    }
    pub fn send_to(&self, bytes: &[u8], addr: SocketAddr) -> io::Result<usize> {
        if bytes.len() > MAX_PACKET_BYTES
            || addr != address(if self.local.port() == 1 { 2 } else { 1 })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid browser datagram",
            ));
        }
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            // Repeated ACKs can share their queued datagram; keeping a single
            // copy leaves room for new commands under a throttled host drain.
            if b.outgoing.iter().any(|p| p == bytes) {
                return Ok(bytes.len());
            }
            if b.outgoing.len() == QUEUE {
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            b.outgoing.push_back(bytes.to_vec());
            Ok(bytes.len())
        })
    }
    pub fn recv_from(
        &self,
        buffer: &mut [u8; MAX_PACKET_BYTES + 1],
    ) -> io::Result<Option<(usize, SocketAddr)>> {
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            let Some(bytes) = b.incoming.pop_front() else {
                return Ok(None);
            };
            let n = bytes.len();
            buffer[..n].copy_from_slice(&bytes);
            Ok(Some((
                n,
                address(if self.local.port() == 1 { 2 } else { 1 }),
            )))
        })
    }
}
