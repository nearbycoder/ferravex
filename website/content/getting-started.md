# Start building with Ferravex

Ferravex is an original Rust engine for compact 2D and 3D games. Start with a working example, then choose the engine systems your game needs.

## Clone the engine

Install a stable Rust toolchain, Git and Python 3. Then clone the repository:

```sh
git clone https://github.com/nearbycoder/ferravex.git
cd ferravex
cargo test --workspace
```

The source is MIT licensed. Every engine crate, tool and example uses only project-owned workspace crates and Rust std. The audit covers all optional features and platforms.

## Play in your browser

Every example has a playable page in the [example collection](/examples/). No installation is required to play. Load one game at a time; keyboard, pointer lock and audio require interacting with its play surface.

To build and serve the browser examples locally:

```sh
rustup target add wasm32-unknown-unknown
sh scripts/build-web.sh release
python3 scripts/serve-web.py --port 8000
```

Open http://127.0.0.1:8000/owned/ for Courtyard or http://127.0.0.1:8000/owned-orbit/ for Orbit Breaker.

## Run a native simulation

The owned native hosts currently run headless and export screenshots:

```sh
cargo run --release -p owned-lab -- --output /tmp/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --frames 1800 --screenshot /tmp/orbit.png
```

Native hosts currently provide simulation and PNG export. Native window/GPU/audio bindings remain future work. Windows/macOS/iOS compilation does not establish interactive device support.

## Add Ferravex to a Rust project

Ferravex has not yet been published to crates.io. Use the Git dependency:

```toml
[dependencies]
ferravex = { git = "https://github.com/nearbycoder/ferravex", package = "ferravex" }
```

Or point at a local checkout:

```toml
[dependencies]
ferravex = { path = "../ferravex/crates/ferravex" }
```

The facade exposes math, ECS, dynamics, graphics, gameplay, input, audio, assets, profiling, replay and networking systems. Platform hosts own the frame loop.

## Keep the dependency boundary visible

```sh
python3 scripts/audit-owned.py
```

This checks Cargo's complete resolved workspace across all features and platforms. There are no optional external backends. See the [dependency policy](/docs/dependencies/) and [implementation status](/docs/status/) for the exact boundaries.
