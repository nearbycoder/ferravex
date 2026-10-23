# Parent integration: bounded local datagram sessions232

Recovered implementation from the first provider capacity failure, then captured
all3 frozen source/test/example files over accepted231 with only the public
local_link registration. Snapshot SHA-256:
445346eafebc4f7234608cd5b4880c41a67538890603b042865bd586a2511529.
Net145, FPS166 and6 explicit local-session example tests passed. Strict all-target
net/FPS Clippy, facade and net/FPS/facade Wasm checks passed. Parent ran the actual
256-tick impaired/reconnect, packet-cap2 and byte-cap256 inspector recipes; all
exited0. Full commands/output hashes/logs: integration-232/report.json and runs.json.
Default cargo test does not execute example tests; that explicit coverage matters.

Reviewed router time/identity/capacity preflight, packet copies, FIFO due-time
selection, one-removal receives, loss handling, disposal/reset and frame pump.
Public bounded router uses64 peers/4096 packets, initialized1200-byte packet slots,
logical payload byte caps and setup-time fallible reservation. Successful send
means complete LOCAL admission even for later configured loss. Refusal consumes
no impairment ordinal, time or queue capacity. Exact monotonic microsecond time
is supplied; successful empty receive advances it. Output too short is atomic,
even for a loss record. Each receive removes at most one due record; no hidden
retry-until-success or socket/clock/entropy dependency. Queue lookup is bounded
linear work. Payload caps are not RSS or end-to-end performance claims.

Caller namespaces must remain unique relative to surviving endpoint handles.
Reset keeps monotonic time and invalidates generations; endpoint reset preserves
unrelated routes. Dispose removes both directions permanently. Ordinal/generation
exhaustion rejects without wrap. Router handles are routing guards, not wire
authentication. No global IDs, transport protocol/version or prediction copy.

The real example composes one ServerState and two ClientStates, exactly64Hz
supplied boundaries, at most4 authority ticks/frame and bounded pump passes.
Zero-tick/menu frames continue protocol work; local menus mask commands only.
Accepted TickOutput events and validated client drains are the feedback source.
Real tests cover two independent sessions, accepted combat delivery through
loss/reorder, refusal/retry without extra ticks, reconnect and authority retention.
Shutdown maintenance is bounded and not a guarantee to drain arbitrary impairment.
Errors preserve committed driver prefixes, not arbitrary host rollback. Existing
231 drivers/helpers are byte-identical in this graph; root reexport formatting
is separate parent release cleanup. No actionable material regression established
in this scoped review. Headless supplied-time scenarios do not establish browser
rendering, external-network security, physical devices, audio or performance.
