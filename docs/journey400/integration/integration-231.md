# Parent integration: transport-independent prediction client 231

Captured exact returned client/adapters/tests over validated230. Frozen source
SHA-256 dfa0ef809b7e61d29b547fc6693cede854ccf7c4ee0d6ab8f02b264f27fb3e06.
134 networking and166 FPS tests/doctests passed, with no ignored tests. Strict
all-target FPS Clippy, native facade and FPS/facade Wasm checks passed. Exact
commands, source inventory and logs: integration-231/{source-capture,report}.json.
Parent registered public client_state in network and reexported it from FPS;
local_link232 registration happened after this capture and is excluded.

Reviewed actual constructor/time validation, receive/poll/tick state machine,
Welcome/session gates, exact snapshot installation, ACK/recovery, ordered
receipt admission/draining, output refusal, reconnect and production adapters.
The same ClientState now powers native UDP and browser Socket GameClient.
Authority230 bodies, shared prediction math, combat/rewind/replay/spawn/tactical
simulation, browser transport and task240 relocation tests are preserved;
integration-231/preservation.json records byte comparisons. An initial comparison
helper included new client documentation in a server slice; its end marker was
corrected without production changes. No actionable material regression was
established in this scoped review.

Core time is caller-supplied monotonic microsecond Duration, bounded to2^53
microseconds. Production adapters use existing elapsed transport time. Poll and
receive continue through local overlays without creating new input ticks; they
may reconcile previously executed input. Tick creates at most one new sanitized
prediction update. A refused datagram leaves the committed prefix intact: inspect
last_report/tick_executed after errors instead of blindly executing another tick.
Legacy methods and counter access through Deref remain; struct layout,
destructuring and split-field borrow compatibility are not promised.

Output budgets clamp to128 packets/153600 bytes and stop at the first refusal or
nonfitting packet. At most8 ordered applications are drained per operation and8
reliable command sends offered per poll. Input retention is128 newest inputs with
latest6 redundancy. Snapshot/render histories each retain32 entries (at most16
players); exact baseline storage is38400 bytes. Receipt/delivery admission is
bounded, reserves space for a missing ordered head, and ACKs only retained or
legitimately duplicate payloads. Application capacity is checked before consuming
an ordered event. Draining and then polling delivers retained tails without a
new network packet. Complete-datagram acceptance commits retry timestamps; it is
not remote delivery. Latest lobby/round/result are single last values; gameplay
outcomes preserve ordered delivery.

Snapshots preflight session, serial age and local membership, then install exact
full/reconstructed bytes before publishing state/ACK. Missing or invalid deltas
request a full snapshot; accepted request admission gates100ms retry timing.
Clock probe state commits only on accepted output. Strictly more than5s without
an accepted Welcome/snapshot clears session state and requires a fresh host nonce;
the core performs no I/O, entropy acquisition or automatic retry loop. Reconnect
clears prediction, pending inputs, IDs, baselines, histories, clock/probes,
commands/events and ACK tails together. Hosts must avoid reusing older session
nonces; there is no unbounded nonce ledger. Rejection counters remain cumulative.
Valid time can advance the watermark before a later operation error.

Twelve public regressions and six new inline doctests exercised actual authority
and prediction cores with bounded mailboxes, loss/reorder/refusal, reconnect and
real accepted combat outcomes. The executable two-core recipe advances120 ticks;
it does not manufacture outcomes. Existing relocation loopback remains green.
Task232 will provide reusable local transport composition. Logical queue/work
bounds are not a zero-allocation, RSS or latency benchmark: decode/history and
snapshot collections allocate. Legacy absolute-f64 render target precision above
2^53 ticks and serial/life reuse constraints remain. No packet version/bytes,
prediction formulas or authoritative game rules changed. Live game migration,
fresh browser network execution, physical devices and sustained performance are
still separate work. Served296 release is unchanged.
