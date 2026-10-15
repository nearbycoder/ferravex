# Engine iteration series

Work in this order. Each item gets its own pull request and merge. Commit and
merge messages include `[skip ci]`; validation runs locally. Linux and available
browser/Wasm checks are in scope; iOS device/SDK checks are excluded on this host.
Keep implementations first-party and record limitations instead of claiming
unsupported platforms or features are complete.

| # | Improvement | Status |
|---|---|---|
| 1 | Performance regression harness | Merged: [PR 1](https://github.com/nearbycoder/ferravex/pull/1) |
| 2 | Shared retained GPU renderer | Implemented; local checks passed |
| 3 | Visibility and detail management | Pending |
| 4 | Materials and custom shader pipeline | Pending |
| 5 | Shared low-poly lighting | Pending |
| 6 | Reusable character controllers | Pending |
| 7 | More stable rigid-body contacts | Pending |
| 8 | Joints and compound colliders | Pending |
| 9 | Camera rigs | Pending |
| 10 | Unified player input | Pending |
| 11 | Shared runtime audio | Pending |
| 12 | Animation playback and blending | Pending |
| 13 | Reusable effects | Pending |
| 14 | AI perception and decisions | Pending |
| 15 | Data-driven scenes and prefabs | Pending |
| 16 | Shared HUD and menus | Pending |
| 17 | Persistent saves and settings | Pending |
| 18 | Replay, ghosts and debugging | Pending |
| 19 | Multiplayer integration | Pending |
| 20 | Native platform hosts | Pending |

Performance checks: [PERFORMANCE.md](PERFORMANCE.md). Raw reports and baselines
live in ignored `target/perf`, so they do not inflate the engine distribution.
