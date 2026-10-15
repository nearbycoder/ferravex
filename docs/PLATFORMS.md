# Platforms

## Current support boundary

| Platform | Build path | Runtime scope |
| --- | --- | --- |
| Web | wasm32-unknown-unknown | Interactive Courtyard through WebGL2 and Orbit Breaker through Canvas2D |
| Linux | Native Rust | Headless simulations, UDP server, tools and PNG export |
| Windows | Native Rust | Headless simulations, UDP server, tools and PNG export; CI covers transport behavior |
| macOS | Native Rust | Headless simulations, UDP server, tools and PNG export |
| iOS | aarch64-apple-ios / aarch64-apple-ios-sim | Rust compile checks; native interactive host and device validation remain pending |

Every target uses the same first-party crate graph. There are no optional external rendering or physics packages. Browser game rules remain in Rust; hosts bind the standard browser APIs directly. OS libraries, Rust std and browser implementations are the platform boundary.

## Browser builds

```sh
rustup target add wasm32-unknown-unknown
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Open /owned/ or /owned-orbit/. The builder clears dist/web before staging the two catalog entries, their original JavaScript/GLSL hosts and a build manifest. It does not fetch loaders or runtime packages. The small profile is available through python3 scripts/build-web.py small; measure throughput as well as bytes before choosing it.

## Native builds

```sh
cargo build --workspace --all-targets --all-features --offline --locked
cargo run --release -p owned-lab -- --output target/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --screenshot target/orbit.png
```

These examples run headless. Native window creation, GPU presentation, platform audio sinks and lifecycle/input FFI bindings remain future work. No native app bundle or windowed game is implied by successful compilation.

## Apple compile checks

On macOS with the Apple SDK:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo check --workspace --all-features --offline --locked --target aarch64-apple-ios
cargo check --workspace --all-features --offline --locked --target aarch64-apple-ios-sim
```

CI also checks native macOS, Windows, Linux and WebAssembly. An SDK compile does not verify signing, installation, touch ergonomics, graphics drivers or runtime lifecycle behavior on a physical device.

## Artifact sizes

```sh
python3 scripts/report-size.py --profile release
python3 scripts/report-size.py --profile release --json
```

The report measures native binaries and the two WebAssembly modules, with their original host files listed separately. Missing artifacts are labeled. Gzip is a transfer estimate. The production docs server serves precompressed representations; the development web server serves ordinary files.
