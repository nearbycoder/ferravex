# Orchard Guard

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A warm, procedural 2D tower-defense game built on Ferravex. Protect an orchard through four increasingly crowded waves of beetles. The winding dirt path, fruit trees, farmhouse, towers and pests use simple vector geometry with no downloaded assets.

```sh
cargo run --release -p orchard-guard
```

Click or tap one of the eight round plots, then choose a tower in the bottom bar. Start a wave when ready; you can keep building and upgrading while pests move. The orchard survives as long as at least one of its 12 hearts remains after wave four.

| Control | Action |
| --- | --- |
| Click / tap plot | Select a build or upgrade location |
| Click / tap bottom buttons | Build, upgrade, send a wave, or restart |
| 1 / 2 | Build seed sling / frost hive on the selected plot |
| U | Upgrade the selected tower |
| Space | Send the next wave |
| R | Restart the match |

Seed slings cost 60 and fire quickly for direct damage. Frost hives cost 80, deal lighter damage, and slow a pest to 45% speed for 1.65 seconds. Both have three levels; upgrades increase range and power while reducing reload time. Each pest stopped earns 13, and completing a wave earns 45. Escaped pests cost one heart. Pests cannot be blocked, so cover multiple bends and combine frost with seed slings.

The game shares Ferravex's 60 Hz `FixedClock`, aspect-preserving `Canvas2D` for drawing and pointer input, portable renderer, and native `Demo` smoke harness. The simulation is deterministic, with at most 32 live pests and 96 projectiles. Every projectile tracks a unique pest ID, and invalidated targets remove their projectile. No allocations grow with elapsed play time beyond these reserved entity capacities.

```sh
cargo test -p orchard-guard
cargo clippy -p orchard-guard --all-targets -- -D warnings
cargo check -p orchard-guard --target wasm32-unknown-unknown
cargo run --release -p orchard-guard -- --autoplay --smoke-frames 420 --screenshot orchard.png
```

`--autoplay` executes the same purchases, upgrades, wave launches, and combat rules as human play. The complete-match test checks a four-wave win, entity bounds, both tower types and upgrades; separate tests cover failed purchases and a defenseless loss. Native smoke flags use a deterministic render timestep; browser launches are interactive.
