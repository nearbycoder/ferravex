//! Bounded binary WebSocket transport for native/browser development sessions.
//! First-party RFC 6455 handshake/framing; no extensions, text messages or TLS.
//! The handshake SHA-1 is required by the protocol, never used for authentication.
//! References: <https://www.rfc-editor.org/rfc/rfc6455> and <https://www.rfc-editor.org/rfc/rfc3174>.
#[cfg(not(target_arch = "wasm32"))]
mod gateway;
use crate::MAX_PACKET_BYTES;
#[cfg(not(target_arch = "wasm32"))]
pub use gateway::{Config, Gateway, Stats};
const MAX_BUFFER: usize = 8192;
const MAX_HEADER: usize = 4096;
fn digit(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for row in bytes.chunks(3) {
        let a = row[0];
        let b = row.get(1).copied().unwrap_or(0);
        let c = row.get(2).copied().unwrap_or(0);
        for v in [
            ABC[(a >> 2) as usize],
            ABC[((a & 3) << 4 | b >> 4) as usize],
            if row.len() > 1 {
                ABC[((b & 15) << 2 | c >> 6) as usize]
            } else {
                b'='
            },
            if row.len() > 2 {
                ABC[(c & 63) as usize]
            } else {
                b'='
            },
        ] {
            out.push(char::from(v));
        }
    }
    out
}
fn sha1(bytes: &[u8]) -> [u8; 20] {
    let mut padded = Vec::with_capacity((bytes.len() + 9).div_ceil(64) * 64);
    padded.extend(bytes);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend((bytes.len() as u64 * 8).to_be_bytes());
    let mut h = [
        0x67452301u32,
        0xefcdab89,
        0x98badcfe,
        0x10325476,
        0xc3d2e1f0,
    ];
    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, row) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(row.try_into().unwrap());
        }
        for t in 16..80 {
            w[t] = (w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (t, word) in w.into_iter().enumerate() {
            let (f, k) = match t {
                0..=19 => ((b & c) | (!b & d), 0x5a827999u32),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (state, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *state = state.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (row, word) in out.chunks_exact_mut(4).zip(h) {
        row.copy_from_slice(&word.to_be_bytes());
    }
    out
}
fn handshake(bytes: &[u8], origins: &[String]) -> Result<Option<(usize, Vec<u8>)>, u16> {
    let Some(end) = bytes
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .map(|n| n + 4)
    else {
        return if bytes.len() > MAX_HEADER {
            Err(431)
        } else {
            Ok(None)
        };
    };
    if end > MAX_HEADER {
        return Err(431);
    }
    let request = std::str::from_utf8(&bytes[..end]).map_err(|_| 400u16)?;
    let mut lines = request.split("\r\n");
    let mut first = lines.next().ok_or(400u16)?.split(' ');
    if first.next() != Some("GET") {
        return Err(400);
    }
    if !matches!(first.next(), Some("/") | Some("/ferravex")) {
        return Err(404);
    }
    if first.next() != Some("HTTP/1.1") || first.next().is_some() {
        return Err(400);
    }
    let mut headers: Vec<(&str, &str)> = Vec::with_capacity(16);
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or(400u16)?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b < 32 && b != b'\t' || b == 127)
            || headers.len() == 64
            || headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            return Err(400);
        }
        headers.push((name, value.trim()));
    }
    let get = |key: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    };
    let token = |value: Option<&str>, wanted: &str| {
        value.is_some_and(|v| v.split(',').any(|s| s.trim().eq_ignore_ascii_case(wanted)))
    };
    if get("Host").is_none_or(str::is_empty)
        || !token(get("Connection"), "upgrade")
        || !token(get("Upgrade"), "websocket")
        || get("Sec-WebSocket-Version") != Some("13")
        || get("Transfer-Encoding").is_some()
        || get("Content-Length").is_some_and(|v| v != "0")
    {
        return Err(400);
    }
    if let Some(origin) = get("Origin")
        && !origins.iter().any(|o| o == origin)
    {
        return Err(403);
    }
    let key = get("Sec-WebSocket-Key").ok_or(400u16)?;
    let raw = key.as_bytes();
    if raw.len() != 24
        || &raw[22..] != b"=="
        || raw[..22].iter().any(|b| digit(*b).is_none())
        || !digit(raw[21]).unwrap().is_multiple_of(16)
    {
        return Err(400);
    }
    let protocol = if let Some(p) = get("Sec-WebSocket-Protocol") {
        if !p.split(',').any(|v| v.trim() == "ferravex.v4") {
            return Err(400);
        }
        "Sec-WebSocket-Protocol: ferravex.v4\r\n"
    } else {
        ""
    };
    let mut input = Vec::with_capacity(60);
    input.extend(raw);
    input.extend(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let accept = base64(&sha1(&input));
    Ok(Some((end,format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{protocol}\r\n").into_bytes())))
}
#[derive(Debug, PartialEq, Eq)]
enum Message {
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    Close(Vec<u8>),
}
#[derive(Default)]
struct Decoder {
    bytes: Vec<u8>,
    fragment: Vec<u8>,
    active: bool,
    fragments: usize,
}
impl Decoder {
    fn append(&mut self, bytes: &[u8]) -> Result<(), u16> {
        if bytes.len() > MAX_BUFFER - self.bytes.len() {
            return Err(1009);
        }
        self.bytes.extend(bytes);
        Ok(())
    }
    fn next(&mut self) -> Result<Option<Message>, u16> {
        loop {
            if self.bytes.len() < 2 {
                return Ok(None);
            }
            let a = self.bytes[0];
            let b = self.bytes[1];
            let fin = a & 128 != 0;
            let opcode = a & 15;
            if a & 0x70 != 0 || b & 128 == 0 || !matches!(opcode, 0 | 1 | 2 | 8 | 9 | 10) {
                return Err(1002);
            }
            let control = opcode >= 8;
            if control && !fin {
                return Err(1002);
            }
            let (len, head) = match b & 127 {
                126 => {
                    if self.bytes.len() < 4 {
                        return Ok(None);
                    }
                    let n = u16::from_be_bytes(self.bytes[2..4].try_into().unwrap()) as usize;
                    if n < 126 {
                        return Err(1002);
                    }
                    (n, 4)
                }
                127 => {
                    if self.bytes.len() < 10 {
                        return Ok(None);
                    }
                    let n = u64::from_be_bytes(self.bytes[2..10].try_into().unwrap());
                    if n < 65536 || n >> 63 != 0 {
                        return Err(1002);
                    }
                    return Err(1009);
                }
                n => (n as usize, 2),
            };
            if control && len > 125 {
                return Err(1002);
            }
            if !control && (len > MAX_PACKET_BYTES || len > MAX_PACKET_BYTES - self.fragment.len())
            {
                return Err(1009);
            }
            let total = head + 4 + len;
            if self.bytes.len() < total {
                return Ok(None);
            }
            let mask: [u8; 4] = self.bytes[head..head + 4].try_into().unwrap();
            let payload: Vec<u8> = self.bytes[head + 4..total]
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ mask[i % 4])
                .collect();
            self.bytes.drain(..total);
            match opcode {
                0 => {
                    if !self.active {
                        return Err(1002);
                    }
                    self.fragments += 1;
                    if self.fragments > 32 {
                        return Err(1009);
                    }
                    self.fragment.extend(payload);
                    if fin {
                        self.active = false;
                        self.fragments = 0;
                        return Ok(Some(Message::Binary(std::mem::take(&mut self.fragment))));
                    }
                }
                1 => return Err(1003),
                2 => {
                    if self.active {
                        return Err(1002);
                    }
                    if fin {
                        return Ok(Some(Message::Binary(payload)));
                    }
                    self.active = true;
                    self.fragments = 1;
                    self.fragment = payload;
                }
                8 => {
                    if payload.len() == 1 {
                        return Err(1002);
                    }
                    if payload.len() >= 2 {
                        let code = u16::from_be_bytes(payload[..2].try_into().unwrap());
                        if !((1000..=1014).contains(&code) && !matches!(code, 1004..=1006)
                            || (3000..=4999).contains(&code))
                        {
                            return Err(1002);
                        }
                        if std::str::from_utf8(&payload[2..]).is_err() {
                            return Err(1007);
                        }
                    }
                    return Ok(Some(Message::Close(payload)));
                }
                9 => return Ok(Some(Message::Ping(payload))),
                10 => return Ok(Some(Message::Pong)),
                _ => unreachable!(),
            }
        }
    }
}
fn frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    out.push(0x80 | opcode);
    if payload.len() < 126 {
        out.push(payload.len() as u8);
    } else {
        out.push(126);
        out.extend((payload.len() as u16).to_be_bytes());
    }
    out.extend(payload);
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn masked(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
        let mut b = frame(opcode, payload);
        if !fin {
            b[0] &= 127;
        }
        b[1] |= 128;
        let at = if payload.len() < 126 { 2 } else { 4 };
        b.splice(at..at, [1, 2, 3, 4]);
        for (i, v) in b[at + 4..].iter_mut().enumerate() {
            *v ^= [1, 2, 3, 4][i % 4];
        }
        b
    }
    #[test]
    fn standard_handshake_and_hash_vectors() {
        assert_eq!(
            sha1(b"abc"),
            [
                0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
                0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d
            ]
        );
        let req=b"GET /ferravex HTTP/1.1\r\nHost: localhost:4001\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nOrigin: http://localhost:8087\r\nSec-WebSocket-Protocol: ferravex.v4\r\n\r\n";
        for n in 0..req.len() {
            assert_eq!(
                handshake(&req[..n], &["http://localhost:8087".into()]),
                Ok(None)
            );
        }
        let (used, response) = handshake(req, &["http://localhost:8087".into()])
            .unwrap()
            .unwrap();
        assert_eq!(used, req.len());
        assert!(
            String::from_utf8(response)
                .unwrap()
                .contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        );
        assert_eq!(handshake(req, &[]), Err(403));
        let mut duplicate = req[..req.len() - 2].to_vec();
        duplicate.extend(b"Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n");
        assert_eq!(
            handshake(&duplicate, &["http://localhost:8087".into()]),
            Err(400)
        );
        assert_eq!(handshake(&[b'a'; 4097], &[]), Err(431));
    }
    #[test]
    fn fragmented_binary_with_interleaved_ping_and_arbitrary_tcp_boundaries() {
        let all = [
            masked(2, false, b"hello"),
            masked(9, true, b"ping"),
            masked(0, true, b" world"),
        ]
        .concat();
        let mut d = Decoder::default();
        let mut messages = Vec::new();
        for byte in all {
            d.append(&[byte]).unwrap();
            while let Some(m) = d.next().unwrap() {
                messages.push(m);
            }
        }
        assert_eq!(
            messages,
            vec![
                Message::Ping(b"ping".to_vec()),
                Message::Binary(b"hello world".to_vec())
            ]
        );
        let bytes = masked(2, true, &[7; 1200]);
        let mut d = Decoder::default();
        d.append(&bytes).unwrap();
        assert_eq!(d.next().unwrap(), Some(Message::Binary(vec![7; 1200])));
    }
    #[test]
    fn bad_masks_flags_lengths_control_frames_and_close_utf8_fail_closed() {
        for bytes in [
            vec![0x82, 0],
            vec![0xc2, 0x80],
            vec![0x89, 0xfe, 0, 126],
            vec![0x09, 0x80],
            vec![0x80, 0x80, 0, 0, 0, 0],
            vec![0x82, 0xfe, 0, 1],
            vec![0x82, 0xfe, 4, 177],
        ] {
            let mut d = Decoder::default();
            d.append(&bytes).unwrap();
            assert!(d.next().is_err(), "{bytes:?}");
        }
        for payload in [&[3][..], &[3, 237][..], &[3, 232, 0xff][..]] {
            let mut d = Decoder::default();
            d.append(&masked(8, true, payload)).unwrap();
            assert!(d.next().is_err());
        }
    }
}
