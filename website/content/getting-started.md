# Start building with Ferravex

Ferravex is a small, readable Rust engine for 2D and 3D games. Everything from math and physics to rendering, audio and netcode lives in this repository, so you can follow any behavior straight to its source.

You need Rust 1.88 or newer and Python 3.11 or newer. There is nothing else to install and nothing to download: every Rust dependency is a crate in this repository ([why](../../DEPENDENCIES.md)).

## Make a game in a minute

From the Ferravex checkout:

```sh
python3 -B scripts/ferravex.py new moon-garden
cd ../moon-garden
cargo run
```

`new` creates a playable game next to the checkout, and `cargo run` opens it in a window after a first build of a few seconds. Steer from the starting pad to the gold beacon with WASD or the arrow keys. Add `--template 3d` to `new` for the 3D starter.

The project is a standalone Cargo workspace with its own lockfile and a local `ferravex` dependency; generating it never edits the checkout. Pass a destination as the second argument to put it somewhere else, for example `new moon-garden ~/games/moon-garden`. Optional `--tick-rate HZ` (1 to 1000, default 60) and `--window-title TITLE` flags set the rule rate and the title.

### Make it yours

- `src/game.rs` holds the rules and their tests. Tuning constants such as `SPEED` sit at the top.
- `src/lib.rs` is the frame loop and the world drawing.
- `src/hud.rs` draws the title, timer, key prompts and buttons with the UI kit.

```sh
cargo test                               # rules and doc tests, no window needed
python3 scripts/build-web.py --serve     # play it at http://127.0.0.1:8000/
```

The browser build needs the WebAssembly target once: `rustup target add wasm32-unknown-unknown`. It goes into the project's own `target/web` folder and nothing is deployed. The generated README lists the controls and a few first edits to try.

### If something fails

Native windows need your platform's display and graphics libraries (on Linux: X11 and a working OpenGL driver). To check your tools without launching anything, run these from the checkout:

```sh
python3 -B scripts/ferravex.py doctor --profile core
python3 -B scripts/ferravex.py doctor --profile web
```

[Platforms](../../docs/PLATFORMS.md) covers prerequisites per operating system, and [template verification](../../docs/JOURNEY_200_TEMPLATES.md) describes what the generator is tested against.

## Play the bundled games

Want to see what the engine can do first? `python3 -B scripts/ferravex.py list` shows every bundled game, and `python3 -B scripts/ferravex.py run neon-swarm` runs one natively. To build and serve every browser game from the checkout:

```sh
python3 -B scripts/ferravex.py doctor --profile web
python3 -B scripts/ferravex.py build-web release --jobs 2
python3 -B scripts/serve-web.py --bind 127.0.0.1 --port 8000
```

This needs the WebAssembly target from the previous step. Open http://127.0.0.1:8000/ for the game catalog. Each game has its own route, such as `/emberline/`, `/marble-run/` or `/strike/`; Courtyard lives at `/owned/` and Orbit Breaker at `/owned-orbit/`. On this website the same games live under `/play/`. Load one game at a time, and click the game to give it keyboard focus, pointer lock and audio.

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
