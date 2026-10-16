# Gameplay and reusable engine series

Forty changes, each delivered in its own branch and pull request. Runtime code
uses workspace crates and std only. Simulation owns time, collision, rewards and
accepted input; rendering is an adapter. GitHub Actions stays disabled for this
series; validation is local. The APIs are available under
`ferravex_play::gameplay` and `ferravex_engine::gameplay`.

## Backlog

| # | Feature | Delivery |
| --- | --- | --- |
| 1 | Expiring, single-consumption command buffers | [PR 44](https://github.com/nearbycoder/ferravex/pull/44), merged |
| 2 | Timed score chains | [PR 45](https://github.com/nearbycoder/ferravex/pull/45), merged |
| 3 | Bounded experience and level progression | [PR 46](https://github.com/nearbycoder/ferravex/pull/46), merged |
| 4 | Seeded weighted reward tables | [PR 47](https://github.com/nearbycoder/ferravex/pull/47), merged |
| 5 | Guaranteed pity rewards | [PR 48](https://github.com/nearbycoder/ferravex/pull/48), merged |
| 6 | Distance-based pickup attraction | [PR 49](https://github.com/nearbycoder/ferravex/pull/49), merged |
| 7 | Fixed-capacity item inventories | [PR 50](https://github.com/nearbycoder/ferravex/pull/50), merged |
| 8 | Atomic crafting recipes | [PR 51](https://github.com/nearbycoder/ferravex/pull/51), merged |
| 9 | Capped upgrade ranks and offers | [PR 52](https://github.com/nearbycoder/ferravex/pull/52), merged |
| 10 | Magazine and reload cycles | [PR 53](https://github.com/nearbycoder/ferravex/pull/53), merged |
| 11 | Charge-and-release actions | [PR 54](https://github.com/nearbycoder/ferravex/pull/54), merged |
| 12 | Turn-limited homing steering | [PR 55](https://github.com/nearbycoder/ferravex/pull/55), merged |
| 13 | Pierce and ricochet budgets | [PR 56](https://github.com/nearbycoder/ferravex/pull/56), merged |
| 14 | Safe, deterministic spawn selection | [PR 57](https://github.com/nearbycoder/ferravex/pull/57), merged |
| 15 | Authored wave sequences | [PR 58](https://github.com/nearbycoder/ferravex/pull/58), merged |
| 16 | Interruptible interaction channels | [PR 59](https://github.com/nearbycoder/ferravex/pull/59), merged |
| 17 | Stagger and poise recovery | [PR 60](https://github.com/nearbycoder/ferravex/pull/60), merged |
| 18 | Delayed regenerating shields | [PR 61](https://github.com/nearbycoder/ferravex/pull/61), merged |
| 19 | Monotonic checkpoint state | [PR 62](https://github.com/nearbycoder/ferravex/pull/62), merged |
| 20 | Ordered trial splits and medals | [PR 63](https://github.com/nearbycoder/ferravex/pull/63), merged |
| 21 | Neon Swarm kill-chain scoring | [PR 64](https://github.com/nearbycoder/ferravex/pull/64), merged |
| 22 | Neon Swarm earned weapon progression | [PR 65](https://github.com/nearbycoder/ferravex/pull/65), merged |
| 23 | Neon Swarm magnetic health pickups | [PR 66](https://github.com/nearbycoder/ferravex/pull/66), merged |
| 24 | Neon Swarm safe spawn admission | [PR 67](https://github.com/nearbycoder/ferravex/pull/67), merged |
| 25 | Neon Swarm heavy-enemy stagger | [PR 68](https://github.com/nearbycoder/ferravex/pull/68), merged |
| 26 | Neon Swarm rechargeable player shield | [PR 69](https://github.com/nearbycoder/ferravex/pull/69), merged |
| 27 | Neon Swarm magazine and reload rhythm | [PR 70](https://github.com/nearbycoder/ferravex/pull/70), merged |
| 28 | Neon Swarm buffered dash input | [PR 71](https://github.com/nearbycoder/ferravex/pull/71), merged |
| 29 | Emberline collectible-chain scoring | [PR 72](https://github.com/nearbycoder/ferravex/pull/72), merged |
| 30 | Emberline checkpoint time trials | [PR 73](https://github.com/nearbycoder/ferravex/pull/73), merged |
| 31 | Emberline earned air jump | [PR 74](https://github.com/nearbycoder/ferravex/pull/74), merged |
| 32 | Emberline springboard routes | [PR 75](https://github.com/nearbycoder/ferravex/pull/75), merged |
| 33 | Emberline crumbling bonus platforms | [PR 76](https://github.com/nearbycoder/ferravex/pull/76), merged |
| 34 | Emberline ordered checkpoint recovery | [PR 77](https://github.com/nearbycoder/ferravex/pull/77), merged |
| 35 | Orbit Breaker block-chain scoring | [PR 78](https://github.com/nearbycoder/ferravex/pull/78), merged |
| 36 | Orbit Breaker guaranteed reward drops | [PR 79](https://github.com/nearbycoder/ferravex/pull/79), merged |
| 37 | Orbit Breaker paddle pickup attraction | [PR 80](https://github.com/nearbycoder/ferravex/pull/80), merged |
| 38 | Orbit Breaker opening ball-save window | [PR 81](https://github.com/nearbycoder/ferravex/pull/81), merged |
| 39 | Orbit Breaker charged serves | [PR 82](https://github.com/nearbycoder/ferravex/pull/82), merged |
| 40 | Orbit Breaker precision return rewards | Final feature PR |

Each new module contains behavioral tests and its authoring contract. Game changes
include real-input regressions and retain ordinary victory/loss tests. The delivery links above and validation record below track the completed work.

## Composition guide

[Game-loop recipes](GAMEPLAY_RECIPES.md) map every API to its authoring and authority
contract. The runnable `gameplay_recipe` example composes waves, XP, upgrade offers,
inventory, interaction and atomic crafting without a renderer.

## Validation correction

PR 68's initial local check caught a Rust borrow error. It was merged before that
failed result was handled. PR 69 corrects the borrow, and its local tests, strict
Clippy and native/Wasm checks pass, including actual stagger and victory/loss
regressions. Later feature publication always reruns the local check before push.

The initial Orbit browser input harness also lacked the existing host's
`performance` clock. PR 78 repairs the mock and the routing regression passes.


## Final local validation

Verified on 2026-10-16 with GitHub Actions disabled: all 38 local validation
steps passed, including 630 Rust unit/integration/doc tests, workspace formatting,
strict Clippy, the first-party dependency audit, and the runnable gameplay recipe.

Release native binaries and all fifteen release Wasm games built successfully.
Native smoke runs produced validated PNGs for all fifteen examples at 180 frames,
plus 1,800-frame runs for Neon Swarm, Emberline and Orbit Breaker. All web contract
and real-Wasm suites, native multiplayer checks, both browser input harnesses,
Rustdoc generation, the static site build and Python build-tool tests passed.

The renderer browser input mock now provides the runtime's performance clock and
details settings export; the actual runtime input regression passes unchanged.
Precision-return tests cover real paddle collision rewards, panel-hit eligibility,
empty-bounce farming prevention, capped bonuses, invalid offsets and streak reset.

Live local browser checks confirmed Neon Swarm rendering, Emberline completing
its route with a gold trial medal, and Orbit Breaker launching, scoring from panel
hits and returning to ready after a lost life, with no warning/error console logs.
Windows GNU and Apple ARM macOS targets passed compilation checks; this does not
establish native runtime or physical-device support on those platforms.

Detailed local logs and machine-readable results are in
`target/gameplay40-validation/`; PNGs are in `target/gameplay40-smoke/` and
`target/gameplay40-changed-smoke/`. These generated artifacts are not committed.
