# Engine iteration series

Work in this order. Each item gets its own pull request and merge. Commit and
merge messages include `[skip ci]`; validation runs locally. Linux and available
browser/Wasm checks are in scope; iOS device/SDK checks are excluded on this host.
Keep implementations first-party and record limitations instead of claiming
unsupported platforms or features are complete.

| # | Improvement | Status |
|---|---|---|
| 1 | Performance regression harness | Merged: [PR 1](https://github.com/nearbycoder/ferravex/pull/1) |
| 2 | Shared retained GPU renderer | Merged: [PR 2](https://github.com/nearbycoder/ferravex/pull/2) |
| 3 | Visibility and detail management | Merged: [PR 3](https://github.com/nearbycoder/ferravex/pull/3) |
| 4 | Materials and custom shader pipeline | Merged: [PR 4](https://github.com/nearbycoder/ferravex/pull/4), editor layout fix [PR 5](https://github.com/nearbycoder/ferravex/pull/5) |
| 5 | Shared low-poly lighting | Merged: [PR 6](https://github.com/nearbycoder/ferravex/pull/6) |
| 6 | Reusable character controllers | Merged: [PR 7](https://github.com/nearbycoder/ferravex/pull/7); artifact growth recorded in PERFORMANCE.md |
| 7 | More stable rigid-body contacts | Merged: [PR 8](https://github.com/nearbycoder/ferravex/pull/8); size/work flags recorded in PERFORMANCE.md |
| 8 | Joints and compound colliders | Merged: [PR 9](https://github.com/nearbycoder/ferravex/pull/9); size/work costs recorded in PERFORMANCE.md |
| 9 | Camera rigs | Merged: [PR 10](https://github.com/nearbycoder/ferravex/pull/10); Wasm size costs recorded in PERFORMANCE.md |
| 10 | Unified player input | Implemented; local validation passed; 56 artifact-size flags recorded in PERFORMANCE.md |
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
