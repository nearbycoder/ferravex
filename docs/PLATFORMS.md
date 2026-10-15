# Platforms

## Current support boundary

The restored catalog and hosts are local work. Runtime verification is being repeated after the recovery; prior backend results do not establish support for the new implementation.

| Platform | Build path | Implemented runtime scope |
| --- | --- | --- |
| Web | wasm32-unknown-unknown | Thirteen restored examples through shared WebGL2; Courtyard through its instanced WebGL2 host; Orbit Breaker through Canvas2D |
| Linux | Native Rust | Original X11/GLX window/GPU host for restored examples; headless simulations, UDP server, tools and PNG export |
| Windows | Native Rust | Portable headless simulation/server/tool code; restored interactive game host not implemented |
| macOS | Native Rust | Portable headless simulation/server/tool code; restored interactive game host not implemented |
| iOS | aarch64-apple-ios / aarch64-apple-ios-sim | Portable Rust core targets; app host, signing, lifecycle and device validation not implemented |

Every target uses first-party crates. Browser game rules remain in Rust; original hosts bind browser APIs directly. OS libraries, Rust std and browser implementations are the platform boundary. Native Linux needs system X11 and GLX/OpenGL libraries, a display and a compatible driver. The Linux host implements keyboard, mouse buttons/wheel, focus release, pointer capture with relative motion, resize/quit and GPU screenshots. Native touch, gamepad and audio sinks are not implemented. Window sizes are physical pixels; automatic desktop DPI scaling is not implemented. Other native OS hosts return an explicit unsupported error.

## Browser builds

```sh
rustup target add wasm32-unknown-unknown
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Open http://127.0.0.1:8000/ for the catalog. Individual restored examples use paths such as `/neon-swarm/` and `/marble-run/`; Courtyard uses `/owned/`, and Orbit Breaker uses `/owned-orbit/`. The builder stages fifteen catalog entries and the shared runtime alongside each game's required host files. It fetches no runtime packages or loaders. `python3 scripts/build-web.py small` selects the size-oriented profile; measure throughput as well as bytes before choosing it.

Browser compilation alone does not verify input, touch, pointer lock, graphics drivers or mobile performance. Exercise the game in the browser after changing the host. WebGL2 availability is required for the 3D and restored immediate-renderer examples.

## Native Linux builds

```sh
cargo build --workspace --all-targets --all-features --offline --locked
cargo run --release -p emberline
cargo run --release -p marble-run
cargo run --release -p strike
```

These restored examples open a Linux window. Their smoke harness accepts `--autoplay`, `--smoke-frames N` and `--screenshot path.png` where supported; Strike has its own `--play` smoke path. Native screenshot capture uses the GPU-rendered frame. A running X display is required even for a bounded smoke run.

Courtyard and Orbit Breaker provide separate headless paths on native Rust targets:

```sh
cargo run --release -p owned-lab -- --output target/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --screenshot target/orbit.png
```

## Cross-target checks

On a machine with the relevant toolchain/SDK, compile selected portable libraries or headless packages for the desired target. For example, on macOS with the Apple SDK:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo check -p ferravex --offline --locked --target aarch64-apple-ios
cargo check -p ferravex --offline --locked --target aarch64-apple-ios-sim
```

These are validation commands, not results from the current local recovery. An SDK compile does not verify signing, installation, touch ergonomics, GPU drivers or lifecycle behavior on a physical device. The interactive Windows, macOS and iOS bindings still need implementation; do not treat a portable core check as a windowed game port.

## Artifact sizes

```sh
python3 scripts/report-size.py --profile release
python3 scripts/report-size.py --profile release --json
```

The report follows the catalog and measures available native binaries and WebAssembly modules. Shared/per-game host files are separate from Rust executable bytes. Missing artifacts are labeled; gzip is a transfer estimate. Local browser/site builds do not publish these artifacts.
