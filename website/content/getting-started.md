# Start building with Ferravex

Ferravex is an original Rust engine for compact 2D and 3D games. Start with a working example, then choose the systems your game needs.

The recovered fifteen-example catalog is local work and has not been pushed or deployed. Use the current checkout for the renderer and games described here; a public Git revision or website may still reflect the earlier catalog. Recovery validation is in progress.

## Check your local source

Install a stable Rust toolchain, Git and Python 3. From the current Ferravex checkout:

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
```

The source is MIT licensed. Every engine crate, tool and example uses only project-owned workspace crates and Rust std. The audit covers all optional features and platforms. Linux windowed games additionally need system X11/GLX libraries and a working display/graphics driver.

## Play in your browser

Build and serve the catalog locally:

```sh
rustup target add wasm32-unknown-unknown
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Open http://127.0.0.1:8000/ to choose an example. Restored pages include `/emberline/`, `/neon-swarm/`, `/marble-run/` and `/strike/`; Courtyard is `/owned/` and Orbit Breaker is `/owned-orbit/`. Load one game at a time. Keyboard focus, pointer lock and audio may require interaction with its play surface.

The thirteen restored examples share an original WebGL2 host. Courtyard retains its separate instanced WebGL2 renderer, and Orbit Breaker presents its Rust framebuffer through Canvas2D. No JavaScript libraries, external fonts or runtime loaders are fetched.

## Run a native game or simulation

The restored games open an original Linux X11/GLX window:

```sh
cargo run --release -p neon-swarm
cargo run --release -p marble-run
```

Courtyard and Orbit Breaker retain headless native screenshot paths:

```sh
cargo run --release -p owned-lab -- --output /tmp/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --frames 1800 --screenshot /tmp/orbit.png
```

Windows/macOS/iOS interactive hosts and native audio output remain unimplemented. Portable Rust core compilation is distinct from interactive device support. See [platform status](/docs/platforms/) for the boundary.

## Add this checkout to a Rust project

Point at local crates so your project uses the recovered implementation:

```toml
[dependencies]
ferravex = { path = "../ferravex/crates/ferravex" }
ferravex-render = { path = "../ferravex/crates/ferravex-render" }
```

The facade exposes math, ECS, translational dynamics, software graphics, gameplay, input, audio, assets, profiling, replay and networking. The immediate renderer owns drawing/input/frame APIs and platform submission. Physics-based restored games also depend directly on `ferravex-physics`. Copy the package structure of a restored example when adding a windowed/browser game; keep gameplay in Rust and hosts limited to platform APIs.

## Keep the dependency boundary visible

```sh
python3 scripts/audit-owned.py
```

This checks Cargo's complete resolved workspace across all features and platforms. There are no optional external backends. See the [dependency policy](/docs/dependencies/) and [implementation status](/docs/status/) for the exact boundaries.

## Browser player tools

Every example includes Details and Controls. The [browser player guide](../../docs/PLAYER_GUIDE.md) explains screenshots, frame limits, accessibility options, settings backups and the limits of browser APIs.
