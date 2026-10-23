# Parent integration: transport-independent authority 230

Captured all five exact returned source/test files over validated240, adding
only the public network::server_state reexport. Source SHA-256:
`537bd4520146789a82af548403cab51e270709c7747ac94d1d89d226b99526e5`.
Full provenance and commands: integration-230/source-capture.json/report.json.
All134 net and147 FPS tests/doctests pass, with no ignored tests. Strict
all-target FPS Clippy, native facade and FPS/facade Wasm checks pass. Includes
all preserved UDP tests, the actual socket-free production-core recipe,
backpressure/ordered-command tests and240's relocation/interpolation loopback.

Parent compared the complete client impl spans in network/replication/tactical,
including shared prediction: byte-identical to the pre-extraction base. Combat,
rewind, replay, spawn, tactical simulation and task240's test are also identical
(preservation.json). Both existing Socket adapters now contain the same one
ServerState; there is no parallel server implementation or new game loop.

Reviewed output admission and retry commitment, exact snapshot retention/ACK
selection, supplied-time validation, session churn, input admission/held expiry,
durable tactical command/result ordering and failure reports. The core uses
one datagram per receive and bounded peer/window/work budgets. Native and browser
adapters retain host clock/entropy and I/O, using the existing microsecond
transport Instant rather than an unrounded OS nanosecond Duration. The core
rejects backwards/sub-microsecond time and invalid cookies explicitly.

Full datagram admission is true only for a complete local send; WouldBlock is
false, and short/fatal socket errors remain host errors after reporting core
work. Accepted external sends are not rolled back. A refused snapshot never
becomes an ACK-authorizable base; delta output retains its exact full encoded
source snapshot. ACKed commands are durably retained before execution; result
space is checked before applying rules, with a completed result retained before
consuming the ordered command. Poll resumes draining even without new ingress.

Output refusal may accompany a completed tick. Later encode/retention errors
may also follow an executed tick/action, so callers must inspect last_report
and must not retry committed simulation as if it rolled back. Reports are per
core call, not an aggregate over GameServer::poll's multiple receives. Time
watermark advances on valid time even if later work fails. Public raw Match
mutations retain their prior validity obligations. Logical bounds are not RSS,
throughput or zero-allocation claims. A callback that sends then returns false
or panics can cause duplicates; its external effects are caller-owned.

Existing five-second timeout,250ms Hello gate, twelve-tick held-input expiry,
practice/tactical rules and protocol v5 remain. Hello admission is gated even
when Welcome output is refused; fresh nonce/session clears peer protocol state
while preserving the real actor. Complete serial reuse and low32 life aliases
remain outer-session constraints, not newly solved wire problems. The public
GameServer field access is through Deref; layout/destructuring and simultaneous
borrow patterns are not promised identical, as documented in the owner receipt.

No actionable material regression established in this scoped review. Parent
registered client_state separately AFTER this immutable capture and assigned
231, with full preservation boundaries and the closed parent-check handoff.
Client WouldBlock/retry, baseline/clock/event migration remain231 work; they
are not claimed implemented by230. No Internet, real browser networking,
allocation-free, physical-device, game-adoption or full-platform claim follows.
The served demo release remains the separate295 bridge gate, not this FPS
library snapshot.
