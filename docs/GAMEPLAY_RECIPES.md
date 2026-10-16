# Build game loops from small rules

Use `ferravex_play::gameplay` or `ferravex_engine::gameplay`. All systems use
first-party Rust and std, independent of rendering/ECS/platform services. Configure
rules at setup, advance them using the simulation clock, and apply their accepted
results through the host's authority. Constructors validate limits; invalid
runtime input preserves state. `LoopError` distinguishes invalid data, exhausted
capacity and unavailable actions.

## Start with a runnable recipe

```sh
cargo run -p ferravex-play --example gameplay_recipe --offline --locked
```

The [example](../crates/ferravex-play/examples/gameplay_recipe.rs) takes a six-enemy,
two-wave specification, produces XP and upgrades, awards wave materials, then
crafts a reward after a workbench interaction. Its tiny headless host immediately
confirms defeats to show composition; replace that part with actual actors and
collision. The three game integrations demonstrate actual physics and controls.

## Pick rules for the game

| Need | API | Host responsibility |
| --- | --- | --- |
| Forgiving actions | `buffer::CommandBuffer<T>` | Push press edges; consume only when executable |
| Chained rewards | `combo::ScoreChain` | Supply accepted, deduplicated events; break on damage/misses |
| Earned progression | `progression::Progression` | Apply returned promotions once |
| Varied loot | `loot::{RewardTable<N>,RewardRng}` | Own seed and stable reward IDs |
| Bounded reward droughts | `pity::PityCounter` | Count eligible events only |
| Pickup attraction | `pickup::Magnet` | Resolve walls and accepted collection |
| Items and currencies | `inventory::Inventory<N>` | Define item IDs and consistent stack limits |
| Crafting | `crafting::craft` | Supply trusted recipe prices, outputs and permissions |
| Roguelike builds | `upgrades::Upgrades<N>` | Spend earned points and define rank effects |
| Reload rhythm | `magazine::Magazine` | Commit damage only when a shot is accepted |
| Heavy attacks | `charge::ChargeAction` | Supply holds/releases; commit costs on returned releases |
| Homing | `homing::steer` | Supply a perceived/authorized target and run collision |
| Piercing and ricochets | `projectile::ProjectileBudget<N>` | Confirm hits and use generational target IDs |
| Safe spawning | `spawn::SpawnSelector` | Check world occupancy before admitting a spawn |
| Wave arcs | `waves::WaveSequence<N>` | Track actual live actors; apply clear rewards once |
| Hold interactions | `interaction::Interaction` | Validate range, visibility, permissions and button state |
| Stagger openings | `stagger::Stagger` | Apply allowed interruption and movement restrictions |
| Escape/recovery | `shield::Shield` | Apply returned unabsorbed damage to health |
| Respawn routes | `checkpoint::Checkpoints<T,N>` | Validate anchor geometry and restore world/save state |
| Timed challenges | `trial::Trial<N>` | Trigger ordered splits from actual route crossings |
| Springboards | `platforms::LaunchPad` | Supply real landings; apply returned speed |
| Temporary terrain | `platforms::CrumblePlatform` | Exclude absent surfaces from collision |
| Skill rewards | `skill::PrecisionReturns` | Supply accepted normalized returns and intervening progress |

N-sized inventories, tables, histories, upgrades and routes use fixed storage and
explicit capacity limits. No system queries hidden world state or performs network
I/O. Keep rule IDs/seeds with saved runs. Checkpoint snapshots restore an anchor,
not an entire level; use the existing typed save/objective systems for world state.
Floating-point simulation is not a promise of cross-platform bitwise determinism.

## Actual game hosts

Neon Swarm combines safe spawn selection, abilities, telegraphed attacks, poise,
shields, magazines, XP, upgrades, pickups and score chains. Its versioned record
context separates the changed rules from earlier runs.

Emberline combines the existing swept character motor with collectible chains,
ordered time-trial splits, inventory-earned air jumps, contact-driven springboards,
crumbling bonus terrain and ordered checkpoint anchors. Original ground jump
forgiveness remains in the character motor.

Orbit Breaker combines continuous ball casts with volley chains, seeded/pity drops,
magnetic pickup motion, a limited opening rescue, charged serves and gated precision
returns. Empty bounces cannot farm precision rewards.

Run each game's tests to exercise ordinary-input victory, loss/reset and focused
new-mechanic regressions. `docs/GAMEPLAY_40.md` records the delivery series.
