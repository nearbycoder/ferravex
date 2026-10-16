# Engine improvements: thirty-feature series

These additions extend the engine facade and remain first-party, std-only, and platform independent. Each item is delivered as an independently validated PR, merged in order. Existing game rules are unchanged.

The APIs live in `ferravex_engine` and are also available through `ferravex`. Each module documents its limits; hosts explicitly drive ticks and synchronization boundaries.

## Planned implementation sequence

- [x] 1. Remove bundle assets with accurate decoded-byte accounting.
- [x] 2. Apply multi-asset edits atomically.
- [x] 3. Extract asset directory subtrees with segment-safe matching.
- [x] 4. Report deterministic added, removed, and changed bundle paths.
- [x] 5. Reset profiler history while retaining registered counters.
- [x] 6. Adjust profiler budgets without replacing counter IDs.
- [x] 7. Query arbitrary nearest-rank profiler percentiles.
- [x] 8. Read chronological profiler samples after ring wrap.
- [x] 9. Seek legacy input replays by exact or preceding tick.
- [x] 10. Export bounded tick ranges and truncate replay branches.
- [x] 11. Locate the first mismatched or missing replay state.
- [x] 12. Bound engine event queues with explicit overflow policies.
- [x] 13. Publish double-buffered events at explicit frame boundaries.
- [x] 14. Schedule and cancel deterministic tick-delayed work.
- [x] 15. Resolve stable system dependencies and reject cycles.
- [x] 16. Buffer bounded world mutations until a synchronization point.
- [x] 17. Reuse bounded object slots while rejecting stale handles.
- [x] 18. Track dirty engine indices with bounded dense bit sets.
- [ ] 19. Provide reproducible random streams and unbiased bounded draws.
- [ ] 20. Select weighted engine variants using checked integer weights.
- [ ] 21. Cycle randomized variants without repeats within each cycle.
- [ ] 22. Retain bounded tick snapshots for interpolation and rollback hosts.
- [ ] 23. Control pause and time scale independently of wall time.
- [ ] 24. Share a deterministic per-frame work budget between systems.
- [ ] 25. Smooth scalar presentation values with frame-independent half lives.
- [ ] 26. Query bounded two-dimensional spatial buckets with exact filtering.
- [ ] 27. Filter hierarchical engine tags with segment-safe ancestry.
- [ ] 28. Store typed singleton engine resources without string keys.
- [ ] 29. Validate deferred lifecycle transitions at frame boundaries.
- [ ] 30. Retain severity-filtered bounded engine diagnostic records.

## Local validation

Each change runs engine unit/doc tests, formatting, and strict engine Clippy before publication. The final pass runs workspace tests, strict workspace Clippy, native and WebAssembly builds, browser host regressions, dependency auditing, and site tests. GitHub Actions remains disabled; commits and squash merges include `[skip ci]`. Apple/Windows runtime checks require their respective hosts and are not implied by Linux validation.
