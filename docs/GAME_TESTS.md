# Ten-game engine exercise

Ten separate game implementations exercise five 2D and five 3D designs. They are
small playable examples with objectives, failure or score rules, and restart.
The original Strike prototype and two physics laboratories remain available.

## Games and behavioral coverage

| Package | Gameplay test coverage |
| --- | --- |
| `emberline` | Resting floor contacts, coyote/buffered jumps, hazard/checkpoint respawn, full input-driven completion |
| `neon-swarm` | Swept shots and cover occlusion, movement/dash, healing/death, all three waves cleared |
| `pocket-rally` | Driving and surface behavior, ordered directional gates, shortcut rejection, three-lap finish |
| `salvage-pinball` | Gated launch, contact-only bumper scoring, physical flipper momentum, drain/lives, a full three-ball run |
| `orchard-guard` | Atomic purchases, unattended loss, real-action four-wave victory, bounded enemies/projectiles |
| `marble-run` | Gravity/contact settling, real angular rolling, braking, checkpoint recovery, all pickups and finish |
| `dune-skimmer` | Boost/sand dynamics, directional and height-bounded gates, shortcut recovery, three-lap finish |
| `crystal-cavern` | Cave collision, interaction rules, crystal/switch/exit progression |
| `siege-foundry` | Real ballistic projectiles, rigid-body scoring, ammunition and reset |
| `sky-courier` | Flight controls, ordered delivery gates, collision, delivery progression |

Game READMEs describe exact controls, rules, and limitations. The automated
players issue the same actions as human players; headless gameplay tests run
without a GPU. Rendered smoke tests use the actual game renderer and simulation.
Autoplay frame timing is fixed for reproducibility, so smoke wall times must not
be presented as performance benchmarks.

## Shared additions and fixes

- `ferravex_render::canvas::Canvas2D` provides one aspect-preserving drawing and
  pointer transform. Visual checks of the 2D games exposed inverted Y coordinates
  and logical-pixel/framebuffer viewport mismatches. The helper uses camera zoom
  instead of raw GPU viewport coordinates; resize/letterbox mapping has unit tests.
- `ferravex_render::input::logical_touches` converts physical touch coordinates to
  logical pixels before hit testing. All touch-enabled games and Strike use it;
  tests cover a portrait canvas at DPI scales 1, 1.5, 2, and 3.
- `ferravex_core::collision::Aabb2` distinguishes positive-area overlap from touching
  edges. This prevents a platformer resting on the floor from being pushed sideways.
  Segment/AABB and segment/circle queries prevent fast shots skipping targets and
  allow comparison with the first blocking wall. Emberline, Neon Swarm, and Crystal
  Cavern exercise the shared queries.
- `Mesh3d` now has capped flat-shaded cylinders, cones, and tapered prisms.
  Sphere tessellation also stays within the renderer batch limit to prevent
  partially drawn high-resolution spheres. Dimension validation, bounded tessellation, outward winding, and unit normals
  are checked. A tessellated ground-plane helper fixes large surfaces fading
  entirely into fog when their only vertices are far from the camera. Its grid
  stays within the default renderer batch capacity. Dune Skimmer and Sky Courier
  exercise it. The 3D examples build reusable meshes at startup.
- `ui::warm_up_font_sizes` prepares each game's declared text sizes before
  drawing, including DPI changes. Browser checks caught a deleted font-atlas
  texture in Siege Foundry; prewarming prevents an atlas resize from invalidating
  queued HUD draws.
- `ferravex_render::demo::Demo` adds common `--autoplay`, `--smoke-frames`, and
  `--screenshot` support. Each game still owns its input strategy and gameplay.
- The example catalog drives Wasm builds, iOS packaging choices, size reports,
  browser navigation, and the collection page. Browser startup uses a HEAD check
  before the loader to avoid fetching the Wasm payload twice. The game picker is
  hidden during keyboard gameplay and opens with Escape. A small edge button
  keeps the picker accessible on touch devices.

Rapier already supplied CCD, body removal, kinematic bodies, contact queries, and
angular dynamics. Pinball, marble rolling, and demolition use those APIs directly;
no replacement physics solver was introduced. Game-specific platforming, driving,
AI, and rules remain in the examples.

## Local verification results

- 90 Rust tests pass, including 43 tests across the ten new games.
- Workspace Clippy passes with warnings denied.
- All thirteen examples build for WebAssembly.
- All ten new games complete 720-frame native render-loop smoke runs.
- Each new game has been opened and visually checked in the browser; the font-atlas
  issue found during this pass was fixed and checked again.

## Reproduce

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --bins --locked
python3 scripts/smoke-games.py --output target/game-validation
sh scripts/build-web.sh release
python3 scripts/serve-web.py
```

Open `http://127.0.0.1:8000/gallery.html`. Individual games use
`http://127.0.0.1:8000/?demo=<package>`. `smoke-games.py` writes captures and a JSON
report, requires each process to exit successfully, and rejects missing/stale
screenshots. Use `--game` repeatedly to test a subset, or `--frames` to change run
length. Linux CI runs the same native renderer under Xvfb.

Local verification covers Linux and browser WebAssembly. CI includes Windows,
macOS, and iOS compilation; those platforms and physical touch devices require
hardware/runtime verification. These examples are offline and silent. Native
multiplayer remains in Strike; the ten new games do not add network transports.
