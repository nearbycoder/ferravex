# Performance and size

Measure the first-party engine with reproducible workloads. Report the machine, toolchain, profile, workload and exclusions with every performance claim.

## Simulation workloads

```sh
cargo run --release -p ferravex-bench -- 4096
cargo run --release -p ferravex-dynamics --example benchmark
```

The workspace benchmark warms each workload for 256 ticks, then measures the requested tick count with std::time::Instant. It reports mean, p50 and p99 for sixteen-player shooter simulation and 256-body scenes in two and three dimensions. The physics uses the original translational solver; angular motion and sleeping are unsupported. Rendering, networking, asset startup and GPU work are excluded. These scenes are regression tools, not predictions of complete game cost on another device.

## Browser and native artifacts

```sh
python3 scripts/build-web.py release
python3 scripts/report-size.py --profile release
cargo run --release -p owned-lab -- --frames 640 --output target/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --frames 1800 --screenshot target/orbit.png
```

Report actual uncompressed file bytes and deterministic gzip bytes separately. A deployment also contains generated API HTML, website guides, screenshots and optional precompressed copies. None of those sizes is the engine's executable size, memory footprint or Docker image size.

The only game modules staged are owned_lab.wasm and owned_orbit.wasm. The site and web builders clear old output before staging, so removed binaries, documentation and loaders do not inflate the current deployment. The default release profile favors throughput; the small profile favors size and requires its own throughput measurements.

## Render measurements

Courtyard's scene tools report CPU mean/p99, draw calls/visible instances and asynchronous GPU timer results where the browser supports them. CPU frame timing includes JavaScript submission and Rust simulation. GPU timing excludes DOM/audio work. Orbit renders into an original Rust RGBA framebuffer and presents it through Canvas2D. Native PNG export timing includes a software render and file write, not a windowed GPU frame.

## Verification

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
python3 scripts/smoke-games.py --output target/game-smoke
```

The dependency audit covers every workspace member, feature and target. Platform/device and production-network limitations are documented in the implementation map and platform guide. Historical measurements of removed backends do not describe this implementation.
