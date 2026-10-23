# Ferravex

An original Rust foundation for compact 2D and 3D games. The workspace uses only first-party crates and Rust std: math, ECS, physics, rendering, gameplay, input, audio, assets, replay and netcode are implemented here. Browser and operating-system APIs form the platform boundary. See the [dependency policy](DEPENDENCIES.md).

Build from this checkout for the current game catalog and browser player tools. Website publication is separate from merging source changes; a deployed site may show an earlier revision. Platform validation limits are documented in [Platforms](docs/PLATFORMS.md). The [Journey400 roadmap](docs/JOURNEY_400.md) records the current stopping point and deferred work.

## Play and build locally

The [catalog](examples/catalog.json) contains fifteen examples: the ten original 2D/3D games, Strike, two physics labs, Courtyard and Orbit Breaker. The thirteen restored examples use the original `ferravex-render` runtime. Courtyard retains its instanced WebGL2 renderer; Orbit Breaker presents its Rust framebuffer through Canvas2D.

```sh
python3 -B scripts/ferravex.py doctor --profile web
python3 -B scripts/audit-owned.py
python3 -B scripts/ferravex.py test -p ferravex-core session
rustup target add wasm32-unknown-unknown
python3 -B scripts/ferravex.py build-web release --jobs 2
python3 -B scripts/serve-web.py --bind 127.0.0.1 --port 8000
```

Open http://127.0.0.1:8000/ for the local catalog. Individual pages include `/emberline/`, `/neon-swarm/`, `/strike/`, `/owned/` for Courtyard and `/owned-orbit/` for Orbit Breaker. Browser hosts use standard APIs directly, with no external runtime libraries, fonts, art downloads or CDN loaders.

Use Rust 1.88 or newer, Python 3.11 or newer, and Node 22 or newer for host tests.
`ferravex.py list` lists catalog IDs; `--dry-run` previews commands. The CLI uses
offline, locked Cargo commands with at most two jobs. Scope `test` and `check`
with `-p`: otherwise they select the workspace. Doctor checks prerequisites,
not gameplay or device support.

## Native games and tools

The restored games use first-party native/window/GPU bindings. Linux uses X11/GLX and needs a working display and OpenGL 3.3 driver; Windows and Apple host source and its verification status are described in [Platforms](docs/PLATFORMS.md):

```sh
python3 -B scripts/ferravex.py run neon-swarm
python3 -B scripts/ferravex.py run marble-run
python3 -B scripts/ferravex.py run strike
```

Courtyard and Orbit Breaker retain their native headless simulation and PNG export paths:

```sh
cargo run --release -p owned-lab --offline --locked -j 2 -- --frames 640 --output target/courtyard.png
cargo run --release -p owned-orbit --offline --locked -j 2 -- --autoplay --frames 1800 --screenshot target/orbit.png
cargo run --release -p ferravex-bench --offline --locked -j 2 -- 4096
cargo run --release -p ferravex-fps --bin ferravex-server --offline --locked -j 2 -- --bind 127.0.0.1:4000
```

The UDP server is for trusted development/LAN experiments. Courtyard has a separate browser WebRTC host/guest adapter with manual offer/answer pairing and no relay or account service. Browser and native network transports do not interoperate automatically. See the [netcode API](crates/ferravex-fps/README.md).

## Use this checkout

The [getting-started guide](website/content/getting-started.md) covers setup and
the standalone 2D/3D generator. From this checkout, create a new sibling project
(the destination must not exist):

```sh
python3 -B scripts/ferravex.py new moon-garden ../moon-garden --template 2d --tick-rate 120 --window-title 'Moon Garden'
cd ../moon-garden
cargo test --offline --locked --jobs 2
```

Use `--template 3d` for a 3D view. The generator seeds a separate workspace and
lockfile, and records provenance in `ferravex.json`. Follow its README for native
rendering and its own `target/web` browser build. Return to the Ferravex checkout
for the remaining repository commands. To use the facade in an existing project,
add this manifest excerpt, adjusting the local path:

```toml
[dependencies]
ferravex = { path = "../ferravex/crates/ferravex" }
```

Start with `use ferravex::prelude::*;` for shared math, input and session types,
then use `render::` for drawing and host functions. The
[facade quickstart](docs/QUICKSTART.md) and its
[source](crates/ferravex/examples/quickstart.rs) demonstrate fixed
updates, interpolated rendering, pause and restart using this single dependency:

```sh
cargo run -p ferravex --example quickstart --offline --locked -j 2
```

That command opens the renderer and requires a working native graphics host.
Append `-- --headless` to run its fixed-tick, paused-step and restart assertions
without opening a window.
[Your first simulation](website/content/first-game.md) is a complete headless
program with executable assertions. `GameSession` coordinates local
start/pause/retry and fixed timing; `ferravex::core::session::SessionConfig::try_new` checks
settings before `GameSession::from_config` creates a session. For a
host-driven application, `ferravex::app` provides explicit startup, fixed-update,
render and shutdown hooks. See the [coding-agent guide](docs/AGENT_GUIDE.md) for
the repository map and development workflow. The
[Journey50 roadmap](docs/JOURNEY_50.md) and
[results](docs/JOURNEY_50_RESULTS.md) document the baseline. The active
[Journey200 ledger](docs/JOURNEY_200.md) records follow-on work and its validation
status. Fresh combined workspace and browser verification is still pending;
implementation receipts are not final integration evidence.

The facade exposes timing, entities, math, ECS, translational dynamics, software graphics, gameplay, input, audio, assets, profiling, replay and networking. The restored examples depend directly on the immediate renderer and, where needed, `ferravex-physics` for rotational rigid bodies. There are no optional external renderer or physics backends.

## Documentation and validation

The documentation navigation retains the earlier guides alongside the Journey200
[API migration cookbook](docs/MIGRATION_200.md),
[gameplay composition](docs/JOURNEY_200_GAMEPLAY.md),
[HUD and presentation composition](docs/JOURNEY_200_PRESENTATION.md),
[save/replay debugging](docs/JOURNEY_200_DEBUGGING.md),
[performance evidence](docs/JOURNEY_200_PERFORMANCE.md),
[accessible gameplay](docs/JOURNEY_200_ACCESSIBILITY.md), and
[template verification](docs/JOURNEY_200_TEMPLATES.md).

Check navigation sources without rebuilding artifacts:

```sh
python3 -B scripts/build-site.py --check-sources
python3 -B scripts/build-site.py --self-test
```

For full integration after concurrent edits settle, the integration owner runs
broader checks and regenerates shared output:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --offline --locked -j 2
cargo clippy --workspace --all-targets --all-features --offline --locked -j 2 -- -D warnings
cargo build --release --workspace --bins --offline --locked -j 2
python3 -B scripts/smoke-games.py --output target/game-smoke
node examples/owned-orbit/tests/platform-input.mjs
CARGO_BUILD_JOBS=2 python3 -B scripts/build-site.py
python3 -B scripts/test_audit_owned.py
python3 -B scripts/test_site.py
python3 -B scripts/serve-site.py --bind 127.0.0.1 --port 8080
```

The full native smoke run needs a Linux X display for the restored games. Use `--game owned-lab --game owned-orbit` for only the headless hosts. Generated web/site output lives in `dist`; these commands build and serve it locally.

This is an engine foundation with original native/browser hosts, shared GPU rendering, audio and development/LAN multiplayer. Historical Windows/macOS cross-target results do not verify the current combined Journey200 revision; runtime/SDK linking and device validation retain their separate limits. Internet services and a Metal backend remain future work. The [implementation map](docs/FROM_SCRATCH.md), [architecture](docs/ARCHITECTURE.md), [validation guide](docs/GAME_TESTS.md) and [platform status](docs/PLATFORMS.md) describe the implemented systems and their limits.

## License

MIT. Engine/game source and authored assets are first-party. Build toolchains, operating systems and browser implementations have their own licenses.

## Compose gameplay quickly

[Gameplay recipes](docs/GAMEPLAY_RECIPES.md) map reusable reward, weapon, encounter, interaction, platform and trial rules to their host contracts. Run `cargo run -p ferravex-play --example gameplay_recipe --offline --locked -j 2` for a small survival/crafting composition. The [40-feature series](docs/GAMEPLAY_40.md) applies the rules to Neon Swarm, Emberline and Orbit Breaker.

## The player update

All fifteen browser examples share the new player tools: frame limits, screenshots, clean sharing, wake lock, active session timing, accessibility preferences and settings backups. Read the [player guide](docs/PLAYER_GUIDE.md) for controls and browser limits. The [30-change catalog](website/releases/player30.json) links each improvement to its individual PR.
