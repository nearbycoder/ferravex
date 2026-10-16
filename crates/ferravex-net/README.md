# ferravex-net

First-party bounded binary codecs, replication/clock/reliable queues, native UDP,
and an optional native WebSocket gateway. No registry dependencies.

## Protocol version 5

Packets begin with `FVRX`, version byte `5`, and a kind byte. Integers and float
bits are little-endian; the maximum packet is 1200 bytes. Truncation, unsupported
versions/kinds, invalid numeric/boolean values and trailing bytes are rejected.
Version 4 and older peers must upgrade together. Version 5 retains the v4 payload layout but rejects older prediction/movement rules. The original input frame remains 21
bytes; full snapshots now include life epochs. Extensions add timed inputs, clock exchange, snapshot ACKs, deltas and
reliable client commands.

| Kind | Direction | Payload |
|---|---|---|
| 1 | Client → server | Hello nonce `u64` |
| 2 | Client → server | Session `u64`, 1–6 original input frames |
| 3 | Server → client | Welcome nonce/session `u64`, player `u8`, tick `u32` |
| 4 | Server → client | Full session/tick/input-ACK, 16 `u32` life epochs, player snapshot |
| 5 | Server → client | Reliable event: session/sequence `u64`, length `u16`, data |
| 6 | Client → server | Event ACK: session/sequence `u64` |
| 7 | Client → server | Session, 1–6 inputs with optional shot tick |
| 8 | Client → server | Session, optional exact snapshot ACK (none requests full) |
| 9 | Client → server | Session, probe nonce `u32`, local send microseconds `u64` |
| 10 | Client → server | Reliable command: session/sequence `u64`, length `u16`, data |
| 11 | Server → client | Session, delta base/current tick `u32`, length/data |
| 12 | Server → client | Session, probe nonce, echoed send/server receive/send microseconds and server tick |
| 13 | Server → client | Command ACK: session/sequence `u64` |

Optional ticks occupy a boolean plus `u32`; absent values require a zero payload.
Session IDs are nonzero. Clock values are bounded to 2^53 microseconds, server
processing to five seconds. Clock replies must match one of four outstanding
probes; duplicate/reordered/expired replies cannot replace accepted estimates.
`clock::ClientClock` exposes RTT, jitter and a bounded render delay.
`clock::TickUnwrapper` maps serial `u32` ticks into ordered history across wrap.
Browser hosts supply monotonic time through `time::set_browser_time`.

The sixteen-player full snapshot is 904 bytes, including per-slot life epochs.
Player jump grace/buffer timers and held state are included for exact motor prediction. Position remains `f32`;
standalone centimeter quantization is available but not used by this session.
`InputQueue` bounds and deduplicates movement, coalesces to the newest sample,
and retains recovered quick-fire aim/time. `clear_pending` keeps the sequence
watermark, preventing old controls from re-entering after a round reset.

`delta::Delta` encodes sparse byte changes with dense full-data fallback. Apply
requires the exact baseline and ordered, unique, in-range changes. The FPS
session integrates 32-packet caches, ACKs and full resynchronization; it chooses
deltas only when smaller. A delta alone is not proof that its base was received.

`reliable::EventSender` caps pending items at 256 and payloads at 900 bytes,
retries after 100 ms, and returns bounded due batches. `EventReceiver` retains a
256-sequence window and delivers once in order. ACK only after successful receive,
including duplicates; individual ACKs never conceal loss of an earlier sequence.
The FPS session uses this in both directions, with bounded command-result credit
and application backpressure. Cookies isolate sessions, not player identities.

## First-party browser gateway

`websocket::Gateway` is a native nonblocking binary WebSocket → UDP adapter.
Call `poll` before/after the authority's fixed tick. Each accepted connection gets
a distinct UDP endpoint, while game semantics remain in the same authority.
It supports RFC 6455 masked client binary frames, bounded fragmentation,
ping/pong, valid close codes/UTF-8, strict upgrade headers, and origin allowlists.
There is no compression, text-message API or native TLS implementation.

The protocol-mandated handshake hash/base64 are implemented locally from the
[RFC 6455 handshake specification](https://www.rfc-editor.org/rfc/rfc6455#section-4)
and [SHA-1 definition](https://www.rfc-editor.org/rfc/rfc3174). SHA-1 is used solely
for that compatibility handshake, not authentication or integrity guarantees.
Golden vectors and real TCP/UDP forwarding tests cover it.

Limits: 16 connections, four accepts/poll, 4 KiB headers, 8 KiB incoming buffer,
1200-byte messages, 32 fragments/message, 64 KiB / 256 queued output frames,
16 received messages/connection/poll, five-second handshake and ten-second idle
deadlines. Browser origins default to local port 8087; explicit extra origins are
configured by the operator. Non-browser clients may omit Origin.

Native UDP/plain WebSocket are LAN/development transports without accounts,
congestion control or native encryption. See [FPS setup and practical limits](../ferravex-fps/README.md).
