# ferravex-play

Original gameplay systems using Rust std and the workspace's first-party scalar math crate. No third-party dependencies, unsafe Rust, renderer requirements, or copied vendor code. Existing scene/animation math values use fixed arrays; camera rigs use the shared math vectors/quaternions.

## Implemented systems

| Module | Working behavior | Current scope |
| --- | --- | --- |
| `persistence` | Bounded versioned typed saves, explicit migrations, presentation preferences and exact personal records | Pure data/validation; native/browser storage and settings integration in ferravex-render. See [saves](../../docs/SAVES.md). |
| `authoring` | Bounded typed scene text, hierarchical prefab instances/overrides, scoped references, component schemas and deterministic cooked data | Load-time validation builds the existing scene graph; optional host reload reconstructs game resources transactionally. See [data-driven scenes](../../docs/SCENES.md). |
| `ai` | Bounded cone/occlusion sight, hearing, expiring memory, priority target selection, shared decision graphs, round-robin sensing and work budgets | Host supplies authoritative candidates/queries; fixed-capacity updates allocate nothing after construction. See [AI](../../docs/AI.md). |
| `audio` | PCM16 WAV decoding/encoding, validated reusable sounds, generational voice handles, pitch/resampling, volume buses, positional panning/falloff, priority stealing, fade/gain control and procedural surface/game cues | Original sample bank generated once. Allocation-free validated playback/mixing; platform output is in ferravex-render. See [audio](../../docs/AUDIO.md). |
| `input` | Bounded named actions, context stacks, held axes/buttons, buffered digital edges and relative motion, radial gamepad response, strict versioned control profiles and multi-touch capture | Platform-independent; renderer hosts supply events, storage and the controls panel. See [input](../../docs/INPUT.md). |
| `camera` | 2D dead-zone follow and 3D follow/orbit/first-person/chase, exponential damping, FOV, cuts, obstruction sphere sweeps and seeded bounded shake | Renderer/input independent. Optional rigid-world adapter is in ferravex-engine; authored boxes use conservative inflation. See [camera rigs](../../docs/CAMERAS.md). |
| `rounds` | Fixed teams, buy phase, weapon/armor/kit purchases, money validation, kill rewards, escalating loss rewards, plant/defuse channels, dropped bomb pickup, death and timeout outcomes, match score, respawn, capped event queue | Host must supply server-validated positions and hit results. This is a reusable rules module, not a transport or collision engine. No halftime, overtime, weapon-specific damage, shop UI, or matchmaking. |
| `navigation` | Deterministic weighted-grid A*, four/eight neighbors, blocked endpoints, no diagonal corner cutting, arrival and separation steering | Incremental indexed-heap A*, FIFO path jobs and polyline/waypoint followers reuse bounded scratch. Convenience `Grid::path` still allocates. No navmesh/crowd solver. Unblocking resets cost to one; edits invalidate active searches. See [AI](../../docs/AI.md). |
| `animation` | Validated scalar/Vec3/quaternion tracks, skeletal and sprite playback, bounded timed events, moving and interrupted crossfades, masked/additive layers and shared deterministic state graphs | Allocation-free updates after construction; no asset-format importer or automatic root-motion extraction. GPU skinning lives in ferravex-render; Strike operators/weapons and Emberline sprites use these players. See [animation](../../docs/ANIMATION.md). |
| `particles` | Fixed-capacity particles, analytic drag/acceleration, counter-seeded moving/burst emitters, distance-sampled fading trails, bounded ribbon miters and short-lived beam pools | CPU simulation/geometry; renderer supplies instanced low-poly billboards and alpha/additive ribbons. No GPU simulation or particle collisions. See [effects](../../docs/EFFECTS.md). |
| `scene` | Generational nodes, local TRS, parent/child hierarchy, cycle rejection, exact world matrices, subtree removal, independent prefab capture/instantiate/clone, strict binary encoding/decoding, skeletal world poses and skin palettes | Matrix composition preserves nonuniform-scale/rotation shear. Skeleton parents must precede children. Host supplies inverse bind matrices. IDs are world-local and remapped during decoding. |
| `ui` | Anchored layout, stretch/center offsets, rectangle intersection, clipped topmost hit testing, pointer capture and activation, cyclic keyboard focus, disabled and removed widget handling | Fixed widget budget, safe-area viewport fitting and allocation-free row/column slots. Shared renderer/text/input adapter in `ferravex-render::ui`; see [HUD and menus](../../docs/UI.md). Screen-reader bridging and complex text shaping remain host work. |

## Rules and boundaries

`TacticalMatch::tick` splits time at interaction and phase deadlines. Bomb expiry wins an exact tie with a completed defuse. Moving more than 15 cm from a plant/defuse origin cancels the channel. Dead attackers do not end a planted-bomb round; elimination of all defenders does. Position updates are accepted authoritative simulation output, not raw client input. Damage is accepted only during live/planted phases between living opposing players. Purchases validate phase, ownership, team, and funds before mutation. Money caps at 16,000. Event history retains the latest 128 events; drain it regularly if every event matters.

The scene format begins with `GSC1`, little-endian version 1 and a node count. Nodes contain a parent index, bounded UTF-8 name, and ten TRS floats. Decoder limits: 16 MiB total, 100,000 nodes, 4,096 bytes per name. It rejects unsupported versions, truncated input, trailing data, invalid UTF-8, non-finite transforms, zero quaternions, invalid parents, and cycles. Parent-graph validation is linear in node count. Scene serialization stores hierarchy and transforms, not arbitrary gameplay components or asset payloads.

Quaternion arrays use `[x, y, z, w]`; matrices are column-major. A skin palette entry is hierarchical model-space joint pose multiplied by its inverse bind matrix. Preallocated `world_pose_into` and `skin_palette_into` avoid per-frame allocations. The retained renderer consumes these palettes through validated `SkinPalette` snapshots.

## Verification

```sh
cargo test -p ferravex-play
cargo clippy -p ferravex-play --all-targets -- -D warnings
```

Unit regressions cover rule transitions/economy, timer partitioning, spatial and team checks, path correctness, weighted detours, corner blocking, animation boundaries and antipodal quaternions, particle bounds and integration, scene identity/cycles/copies/binary validation, bind-pose palettes, and UI focus/capture.

`tests/tactical_scenario.rs` additionally integrates all six modules in one headless host: route an attacker around a blocked grid cell, sample that route as an animation track, update the scene hierarchy, plant the bomb, spawn a particle, activate a defuse button through UI hit testing, finish the match, and round-trip the scene data. This proves subsystem composition without claiming that an existing graphical FPS has been migrated to these systems.
