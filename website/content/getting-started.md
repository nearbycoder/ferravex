# Start building with Ferravex

Ferravex is a first-party Rust foundation for compact 2D and 3D games. Start with a headless simulation, generate a small game, or run one of the fifteen catalog examples from this checkout.

Journey50 remains the baseline. The [Journey200 ledger](../../docs/JOURNEY_200.md) records the follow-on implementation and validation work. Source, generated artifacts and published pages can be different revisions; fresh combined browser and workspace verification is still pending. A guide or implementation receipt does not establish platform support.

## Check your local source

Use a Rust toolchain satisfying the workspace's `rust-version` (currently 1.88), Git and Python 3.11 or newer. Node 22 or newer is used for browser-host tests. Run these commands from the Ferravex checkout:

```sh
python3 -B scripts/ferravex.py doctor --profile core
python3 -B scripts/audit-owned.py
python3 -B scripts/ferravex.py test -p ferravex-core session
python3 -B scripts/ferravex.py list
```

The CLI selects at most two Cargo jobs and uses `--offline --locked`. Always pass `-p` for a focused `test` or `check`: omitting it selects the workspace. Add `--dry-run` to preview a command. Doctor checks prerequisites; it does not play a game.

The Rust graph, including optional, test and platform dependencies, uses only project-owned crates and Rust std. Browser and operating-system APIs are the platform boundary. The documentation site's separately pinned command menu is described in the [dependency policy](../../DEPENDENCIES.md).

## Generate a small game

From the checkout, create a new sibling directory whose parent already exists:

```sh
python3 -B scripts/ferravex.py new moon-garden ../moon-garden --template 2d --tick-rate 120 --window-title 'Moon Garden'
cd ../moon-garden
cargo test --offline --locked --jobs 2
```

The destination must not exist, including as an empty directory or symlink. Use `--template 3d` for a 3D view. Tick rate is finite and between 1 and 1000 Hz; it is independent of rendering frequency. The generator creates a standalone Cargo workspace, a seeded lockfile and a local `ferravex` dependency without editing this checkout's catalog or workspace.

Edit `src/game.rs` for rules and `src/lib.rs` for input, rendering and UI. `ferravex.json` records generation settings and the engine path; changing its tick-rate field alone does not change the compiled game. The generated README lists controls and commands. [Template verification](../../docs/JOURNEY_200_TEMPLATES.md) records the exercised paths and limits.

From the generated project, native rendering needs the platform's display and graphics libraries:

```sh
cargo run --offline --locked --jobs 2
```

For its browser host, install the Wasm standard-library target if needed, then build and serve this project's output:

```sh
rustup target add wasm32-unknown-unknown
python3 -B scripts/build-web.py
python3 -m http.server 8000 --bind 127.0.0.1 --directory target/web
```

Open http://127.0.0.1:8000/ in a WebGL2 browser. This generated-project helper stages `target/web`; the catalog builder below stages `dist/web` instead. Neither command deploys anything.

## Play the existing catalog

Return to the Ferravex checkout before running these commands:

```sh
python3 -B scripts/ferravex.py doctor --profile web
python3 -B scripts/ferravex.py build-web release --jobs 2
python3 -B scripts/serve-web.py --bind 127.0.0.1 --port 8000
```

The Wasm target must already be installed. Open http://127.0.0.1:8000/ for the catalog. Routes include `/emberline/`, `/neon-swarm/`, `/marble-run/` and `/strike/`; Courtyard is `/owned/` and Orbit Breaker is `/owned-orbit/`. The documentation site prefixes these game routes with `/play/`. Load one game at a time; focus, pointer lock and audio may require interaction with its play surface.

Thirteen examples use the shared WebGL2 runtime. Courtyard keeps its separate instanced WebGL2 host; Orbit Breaker presents its Rust framebuffer through Canvas2D. Game hosts use browser APIs directly without third-party runtime libraries or CDN loaders.

For a native game, the CLI resolves the catalog ID and defaults to release:

```sh
python3 -B scripts/ferravex.py run neon-swarm
```

Linux rendering needs X11/GLX and a working OpenGL driver. Windows and Apple host source exists, but its linking and device verification have separate limits. Courtyard and Orbit Breaker also have headless native paths. See [platform status](../../docs/PLATFORMS.md) and each example's README; portable Rust compilation is not interactive device verification.

## Use the actual APIs

The generated project already has its local dependency. For an existing Rust project alongside this checkout, the manifest excerpt is:

```toml
[dependencies]
ferravex = { path = "../ferravex/crates/ferravex" }
```

Start with `ferravex::prelude` for common input, math, camera and session types. Drawing stays under `ferravex::render`; specialized systems keep their module paths. `ferravex::core::session::SessionConfig::try_new` validates session settings, and `GameSession::from_config` creates a session. `ferravex::app` supplies explicit host-driven startup, fixed-update, render and shutdown hooks.

Follow [your first simulation](first-game.md) for a complete headless program and the [facade quickstart](../../docs/QUICKSTART.md) for host composition. Keep simulation on fixed ticks, retain gameplay edges until a tick consumes them, and derive feedback from accepted events. Local controls or menus must not stop an authoritative match or replay clock.

## Choose a focused guide

- [API migration](../../docs/MIGRATION_200.md): compatible additions and existing demo implementations.
- [Gameplay composition](../../docs/JOURNEY_200_GAMEPLAY.md): abilities, objectives, inventory and accepted feedback.
- [Presentation composition](../../docs/JOURNEY_200_PRESENTATION.md): control hints, measured layout, preferences and diagnostics.
- [Save and replay debugging](../../docs/JOURNEY_200_DEBUGGING.md): fingerprints, failures and inspection.
- [Performance evidence](../../docs/JOURNEY_200_PERFORMANCE.md): counters, reproducible measurements and limits.
- [Accessible gameplay](../../docs/JOURNEY_200_ACCESSIBILITY.md): input, independent motion/flash choices and local menus.
- [Coding-agent guide](../../docs/AGENT_GUIDE.md): ownership, API discovery and scoped validation.

Every browser example includes Details and Controls. The [browser player guide](../../docs/PLAYER_GUIDE.md) covers player tools; current settings backups export version 2 and accept strict version 1 or 2 imports. Motion and flash choices are independent. Actual behavior and remaining validation limits belong in the accessibility guide and Journey200 ledger.
