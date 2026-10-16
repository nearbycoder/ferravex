# Engine improvements: thirty-feature series

These additions extend the engine facade and remain first-party, std-only, and platform independent. Each item is delivered as an independently validated PR, merged in order.

The APIs live in `ferravex_engine` and are also available through `ferravex`. Each module documents its limits; hosts explicitly drive ticks and synchronization boundaries.

## Implementation sequence

- [x] 1. Remove bundle assets with accurate decoded-byte accounting. [API source](../crates/ferravex-engine/src/assets.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/84)
- [x] 2. Apply multi-asset edits atomically. [API source](../crates/ferravex-engine/src/assets.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/85)
- [x] 3. Extract asset directory subtrees with segment-safe matching. [API source](../crates/ferravex-engine/src/assets.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/86)
- [x] 4. Report deterministic added, removed, and changed bundle paths. [API source](../crates/ferravex-engine/src/assets.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/87)
- [x] 5. Reset profiler history while retaining registered counters. [API source](../crates/ferravex-engine/src/profile.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/88)
- [x] 6. Adjust profiler budgets without replacing counter IDs. [API source](../crates/ferravex-engine/src/profile.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/89)
- [x] 7. Query arbitrary nearest-rank profiler percentiles. [API source](../crates/ferravex-engine/src/profile.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/90)
- [x] 8. Read chronological profiler samples after ring wrap. [API source](../crates/ferravex-engine/src/profile.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/91)
- [x] 9. Seek legacy input replays by exact or preceding tick. [API source](../crates/ferravex-engine/src/replay.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/92)
- [x] 10. Export bounded tick ranges and truncate replay branches. [API source](../crates/ferravex-engine/src/replay.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/93)
- [x] 11. Locate the first mismatched or missing replay state. [API source](../crates/ferravex-engine/src/replay.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/94)
- [x] 12. Bound engine event queues with explicit overflow policies. [API source](../crates/ferravex-engine/src/events.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/95)
- [x] 13. Publish double-buffered events at explicit frame boundaries. [API source](../crates/ferravex-engine/src/frame_events.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/96)
- [x] 14. Schedule and cancel deterministic tick-delayed work. [API source](../crates/ferravex-engine/src/scheduler.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/97)
- [x] 15. Resolve stable system dependencies and reject cycles. [API source](../crates/ferravex-engine/src/system_order.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/98)
- [x] 16. Buffer bounded world mutations until a synchronization point. [API source](../crates/ferravex-engine/src/commands.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/99)
- [x] 17. Reuse bounded object slots while rejecting stale handles. [API source](../crates/ferravex-engine/src/pool.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/100)
- [x] 18. Track dirty engine indices with bounded dense bit sets. [API source](../crates/ferravex-engine/src/bitset.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/101)
- [x] 19. Provide reproducible random streams and unbiased bounded draws. [API source](../crates/ferravex-engine/src/random.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/102)
- [x] 20. Select weighted engine variants using checked integer weights. [API source](../crates/ferravex-engine/src/weighted.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/103)
- [x] 21. Cycle randomized variants without repeats within each cycle. [API source](../crates/ferravex-engine/src/shuffle.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/104)
- [x] 22. Retain bounded tick snapshots for interpolation and rollback hosts. [API source](../crates/ferravex-engine/src/snapshots.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/105)
- [x] 23. Control pause and time scale independently of wall time. [API source](../crates/ferravex-engine/src/time.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/106)
- [x] 24. Share a deterministic per-frame work budget between systems. [API source](../crates/ferravex-engine/src/work_budget.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/107)
- [x] 25. Smooth scalar presentation values with frame-independent half lives. [API source](../crates/ferravex-engine/src/smoothing.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/108)
- [x] 26. Query bounded two-dimensional spatial buckets with exact filtering. [API source](../crates/ferravex-engine/src/spatial.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/109)
- [x] 27. Filter hierarchical engine tags with segment-safe ancestry. [API source](../crates/ferravex-engine/src/tags.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/110)
- [x] 28. Store typed singleton engine resources without string keys. [API source](../crates/ferravex-engine/src/resources.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/111)
- [x] 29. Validate deferred lifecycle transitions at frame boundaries. [API source](../crates/ferravex-engine/src/state.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/112)
- [x] 30. Retain severity-filtered bounded engine diagnostic records. [API source](../crates/ferravex-engine/src/diagnostics.rs) · [PR](https://github.com/nearbycoder/ferravex/pull/113)

## Local validation

Each change runs engine unit/doc tests, formatting, and strict engine Clippy before publication. The final pass runs workspace tests, strict workspace Clippy, native and WebAssembly builds, browser host regressions, dependency auditing, and site tests. GitHub Actions remains disabled; commits and squash merges include `[skip ci]`. Apple/Windows runtime checks require their respective hosts and are not implied by Linux validation.

## Composing a frame

Run the [headless engine pipeline](../crates/ferravex-engine/examples/engine_pipeline.rs):

```sh
cargo run -p ferravex-engine --example engine_pipeline --all-features --offline --locked
```

The example orders spawn, simulation and publication stages. Tick deadlines enqueue
commands; applying those commands spawns actual ECS entities. Movement updates
feed dirty masks, spatial queries, snapshot history, replay hashes, frame events
and bounded diagnostics. Assertions verify the final state, event overflow,
snapshot eviction, retained asset handles and replay round-trip behavior.

The new APIs are opt-in building blocks. The scheduler resolves deadlines rather
than spawning threads; `SystemOrder` resolves dependencies rather than executing
systems. Deferred commands propagate panics and do not roll back applied work.
Pool and scheduler handles are scoped to their originating instances. Time scaling
feeds a host's simulation clock; it does not read or replace platform time.
Scalar smoothing uses platform `exp` and belongs in presentation, not lockstep
state. Spatial queries index points and do not replace rigid-body collision.

## Verified local result

All 37 validation steps passed on Linux for the final series implementation:

- Workspace target checks, formatting, strict Clippy and the first-party dependency audit.
- 661 Rust tests passed; 14 existing hardware tests remain ignored by the default suite.
- The headless engine pipeline passed all integration assertions.
- Release native builds and rendered smoke runs for all 15 catalog games.
- Release WebAssembly builds for the complete catalog, all 21 browser/Wasm regression suites,
  renderer and Orbit input checks, and native WebSocket integration.
- Artifact-size reporting, Rust API docs, the documentation site and Python script tests.

GitHub Actions stayed disabled throughout. This Linux run does not certify Windows,
Apple or mobile runtime behavior.
