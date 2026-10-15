# ferravex-net

Original dependency-free bounded binary codecs, native UDP sockets and reusable networking building blocks.

## Protocol version 3

Every packet starts with `FVRX`, a one-byte version (`PROTOCOL_VERSION = 3`) and a one-byte kind. Integers and float bit patterns are little-endian. Packets exceeding 1,200 bytes, truncated fields, invalid numeric values, unknown versions/kinds and trailing bytes are rejected. Version 1/2 peers are deliberately incompatible; client and server must upgrade together. Version 3 adds each player's coyote/buffer timers (`f32` seconds) and jump-held state (`u8` boolean), validated to 0–0.5 seconds. The full sixteen-player snapshot occupies 840 bytes, within the packet budget.

| Kind | Direction | Payload after the header |
|---|---|---|
| 1 | Client → server | Hello: nonce `u64` |
| 2 | Client → server | Input: session `u64`, count `u8`, 1–6 existing input frames |
| 3 | Server → client | Welcome: nonce `u64`, session `u64`, player ID `u8`, tick `u32` |
| 4 | Server → client | Full snapshot: session, tick, acknowledged input sequence/validity, bounded player list |
| 5 | Server → client | ReliableEvent: session `u64`, sequence `u64`, payload length `u16`, payload |
| 6 | Client → server | EventAck: session `u64`, individual sequence `u64` |

Reliable event payloads are capped at 900 bytes, so a maximal event packet occupies 924 bytes. Session cookies must be nonzero. The `ferravex-fps` session implements actual transmission, ACKs, retry and consumption; this crate's codec does not send automatically.

`reliable::EventSender` bounds pending events at 256 and retries after 100 ms when polled with a monotonic clock. `EventReceiver` uses a 256-sequence window and returns each delivered event once in order. ACK only when `receive` succeeds, including duplicates. Individual acknowledgments ensure that acknowledging a newer event never hides loss of an older one.

`delta::Delta` provides sparse byte differences with full-payload fallback and exact baseline validation. `history::History` provides bounded ordered samples and interpolation brackets; `ClockSync` provides timestamp offset/RTT estimates. These are standalone building blocks. The current native FPS session does not exchange snapshot-delta baselines, clock-sync timestamps, or client shot times.

UDP is development/LAN transport with no encryption, identity authentication, NAT traversal, congestion controller or internet abuse mitigation. Session cookies isolate connection state; they are not an authentication or encryption protocol.
