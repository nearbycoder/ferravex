# Journey400 206 parent integration

Inspected the registered typed deferred ECS adapter, token/map/queue execution,
owned bundle recovery and lifecycle contract. The minimal engine registration is
present and facade engine exports expose the API without another declaration.
No additional source repair or dependency change was needed. Explicit prepared
spawn budgets and buffer/epoch-local tokens remain separate from world-local
entity handles; one epoch must flush into the same world. Accepted authoring may
allocate, and user callbacks/destructors may retain partial mutations on panic.

Parent package checks passed with all features: 17 public-API behavior/allocation
regressions, five inline lifecycle tests, two runnable plus two compile-fail
doctests, strict scoped Clippy, native facade compilation, and engine/facade Wasm
compilation. The actual tick-boundary and owned-retry recipes execute as doctests.
Exact commands/logs: `integration-206/report.json`; source fingerprints:
`integration-206/source-hashes.json`. Owned formatting and agent-guide links pass.

Agent guide now points to prepared budgets, borrowed-query boundaries, accepted
feedback, mapping claims/reset and allocation limits. No demo is rewritten by
this parent integration: future Neon adoption must preserve its separate AI,
pacing, accepted scoring and reset ownership. No browser/device/performance or
whole-journey integration claim. The current Courtyard web snapshot excludes
concurrent Journey400 changes intentionally.
