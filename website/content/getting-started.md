# Start building with Ferravex

Ferravex is a small, readable Rust engine for 2D and 3D games. Everything from math and physics to rendering, audio and netcode lives in this repository, so you can follow any behavior straight to its source.

This guide takes you from a fresh checkout to a game of your own: check your tools, generate a project, then run the bundled games. Want to see what the engine can do first? [Play the games](../../examples/catalog.json) right in your browser.

## Check your setup

You need:

- Rust 1.88 or newer (the workspace's `rust-version`)
- Git and Python 3.11 or newer
- Node 22 or newer, only if you run the browser-host tests

From the Ferravex checkout, run:

```sh
python3 -B scripts/ferravex.py doctor --profile core
python3 -B scripts/audit-owned.py
python3 -B scripts/ferravex.py test -p ferravex-core session
python3 -B scripts/ferravex.py list
```

`doctor` checks your prerequisites (it does not launch a game) and `list` shows every game you can run. The helper always uses `--offline --locked` and at most two Cargo jobs. Add `--dry-run` to preview any command. Pass `-p` to `test` or `check` to focus on one package; without it they cover the whole workspace.

There is nothing to download: the Rust dependency graph, including optional, test and platform dependencies, contains only crates from this repository and Rust std. The [dependency policy](../../DEPENDENCIES.md) explains the rules, including the one self-hosted script this documentation site uses for search.

## Generate a small game

Create a new project next to your checkout. The destination folder must not exist yet:

```sh
python3 -B scripts/ferravex.py new moon-garden ../moon-garden --template 2d --tick-rate 120 --window-title 'Moon Garden'
cd ../moon-garden
cargo test --offline --locked --jobs 2
```

Use `--template 3d` for a 3D starter. The tick rate (1 to 1000 Hz) sets how often your rules run, independently of the frame rate. The generator creates a standalone Cargo workspace with its own lockfile and a local `ferravex` dependency. It never edits this checkout.

Where to start editing:

- `src/game.rs` holds your rules and game state.
- `src/lib.rs` handles input, rendering and UI.
- `ferravex.json` records how the project was generated. Changing its tick rate does not change the compiled game; edit the code instead.

The generated README lists the controls and commands. [Template verification](../../docs/JOURNEY_200_TEMPLATES.md) describes what the generator is tested against.

### Run it in a window

From the generated project:

```sh
cargo run --offline --locked --jobs 2
```

Native windows need your platform's display and graphics libraries.

### Run it in the browser

Install the WebAssembly target once, then build and serve the project:

```sh
rustup target add wasm32-unknown-unknown
python3 -B scripts/build-web.py
python3 -m http.server 8000 --bind 127.0.0.1 --directory target/web
```

Open http://127.0.0.1:8000/ in a browser with WebGL2. The build goes into the project's own `target/web` folder and nothing is deployed.

## Play the bundled games

Back in the Ferravex checkout, build and serve every browser game:

```sh
python3 -B scripts/ferravex.py doctor --profile web
python3 -B scripts/ferravex.py build-web release --jobs 2
python3 -B scripts/serve-web.py --bind 127.0.0.1 --port 8000
```

This needs the WebAssembly target from the previous step. Open http://127.0.0.1:8000/ for the game catalog. Each game has its own route, such as `/emberline/`, `/marble-run/` or `/strike/`; Courtyard lives at `/owned/` and Orbit Breaker at `/owned-orbit/`. On this website the same games live under `/play/`. Load one game at a time, and click the game to give it keyboard focus, pointer lock and audio.

To run a game natively, pass its catalog ID:

```sh
python3 -B scripts/ferravex.py run neon-swarm
```

On Linux, native rendering needs X11/GLX and a working OpenGL driver. Windows and macOS host code exists with its own verification limits, and Courtyard and Orbit Breaker can also run headless. See [Platforms](../../docs/PLATFORMS.md) and each game's README for details.

Most games share one WebGL2 runtime. Courtyard keeps its own instanced WebGL2 host, and Orbit Breaker draws a Rust framebuffer through Canvas2D. Every game host uses browser APIs directly, with no third-party libraries or CDNs.

## Use Ferravex in your own project

A generated project already depends on Ferravex. To add it to an existing Rust project that sits next to this checkout:

```toml
[dependencies]
ferravex = { path = "../ferravex/crates/ferravex" }
```

Then start with these entry points:

- `use ferravex::prelude::*;` brings in common input, math, camera and session types.
- `ferravex::render` handles drawing. Specialized systems keep their own module paths.
- `ferravex::core::session::SessionConfig::try_new` validates session settings, and `GameSession::from_config` creates the session.
- `ferravex::app` provides explicit startup, fixed-update, render and shutdown hooks.

Four habits keep a Ferravex game predictable:

1. Run gameplay on fixed ticks, never on the frame rate.
2. Hold on to button presses until a tick actually consumes them.
3. Trigger sounds and effects from accepted game events.
4. Let a local menu pause local play only; online matches and replays keep their own clock.

[Your first simulation](first-game.md) is a complete headless program to read next. The [quickstart](../../docs/QUICKSTART.md) adds a window, input and rendering.

## Where to go next

- [Tour the engine](../../docs/ENGINE_WORKFLOWS.md): fixed ticks, entities, physics, responsive UI and networking, with checked examples.
- [Compose a gameplay scenario](../../docs/JOURNEY_200_GAMEPLAY.md): abilities, objectives, inventory and event-driven feedback.
- [Build a HUD and presentation](../../docs/JOURNEY_200_PRESENTATION.md): control hints, measured layout and player preferences.
- [Debug saves and replays](../../docs/JOURNEY_200_DEBUGGING.md): fingerprints, failures and inspection.
- [Measure performance](../../docs/JOURNEY_200_PERFORMANCE.md): counters and reproducible measurements.
- [Accessible gameplay](../../docs/JOURNEY_200_ACCESSIBILITY.md): input, motion and flash choices, and local menus.
- [Adopt the newer APIs](../../docs/MIGRATION_200.md): additions you can bring into an existing game.
- [Coding-agent guide](../../docs/AGENT_GUIDE.md): give an agent the repository map and a way to verify its work.

Here to play rather than build? The [browser player guide](../../docs/PLAYER_GUIDE.md) covers controls, settings, screenshots and troubleshooting for every game.
