# Gameplay and reusable engine series

Forty changes, each delivered in its own branch and pull request. Runtime code
uses workspace crates and std only. Simulation owns time, collision, rewards and
accepted input; rendering is an adapter. GitHub Actions stays disabled for this
series; validation is local. The APIs are available under
`ferravex_play::gameplay` and `ferravex_engine::gameplay`.

## Backlog

| # | Feature |
| --- | --- |
| 1 | Expiring, single-consumption command buffers |
| 2 | Timed score chains |
| 3 | Bounded experience and level progression |
| 4 | Seeded weighted reward tables |
| 5 | Guaranteed pity rewards |
| 6 | Distance-based pickup attraction |
| 7 | Fixed-capacity item inventories |
| 8 | Atomic crafting recipes |
| 9 | Capped upgrade ranks and offers |
| 10 | Magazine and reload cycles |
| 11 | Charge-and-release actions |
| 12 | Turn-limited homing steering |
| 13 | Pierce and ricochet budgets |
| 14 | Safe, deterministic spawn selection |
| 15 | Authored wave sequences |
| 16 | Interruptible interaction channels |
| 17 | Stagger and poise recovery |
| 18 | Delayed regenerating shields |
| 19 | Monotonic checkpoint state |
| 20 | Ordered trial splits and medals |
| 21 | Neon Swarm kill-chain scoring |
| 22 | Neon Swarm earned weapon progression |
| 23 | Neon Swarm magnetic health pickups |
| 24 | Neon Swarm safe spawn admission |
| 25 | Neon Swarm heavy-enemy stagger |
| 26 | Neon Swarm rechargeable player shield |
| 27 | Neon Swarm magazine and reload rhythm |
| 28 | Neon Swarm buffered dash input |
| 29 | Emberline collectible-chain scoring |
| 30 | Emberline checkpoint time trials |
| 31 | Emberline earned air jump |
| 32 | Emberline springboard routes |
| 33 | Emberline crumbling bonus platforms |
| 34 | Emberline ordered checkpoint recovery |
| 35 | Orbit Breaker block-chain scoring |
| 36 | Orbit Breaker guaranteed reward drops |
| 37 | Orbit Breaker paddle pickup attraction |
| 38 | Orbit Breaker opening ball-save window |
| 39 | Orbit Breaker charged serves |
| 40 | Orbit Breaker precision return rewards |

Each new module contains behavioral tests and its authoring contract. Game changes
include real-input regressions and retain ordinary victory/loss tests. The final
validation record and merged PR links are appended after completion.
