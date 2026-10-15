# Ferravex

[Website](https://ferravex.com) · [Documentation](https://ferravex.com/docs/getting-started/) · [Examples](https://ferravex.com/examples/) · [Rust API](https://ferravex.com/api/ferravex/index.html)

An original Rust foundation for compact 2D and 3D games. **The entire Cargo workspace has zero external packages**, including optional features, tools and examples. Math, ECS, translational physics, meshes, rendering, gameplay, input, audio, assets, replay and netcode are implemented here. Rust std and browser/system APIs form the platform boundary. See the [dependency policy](DEPENDENCIES.md).

## Play and build

The two playable games are [Courtyard](examples/owned-lab), a low-poly 3D shooter training ground, and [Orbit Breaker](examples/owned-orbit), a 2D arcade physics game.

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
rustup target add wasm32-unknown-unknown
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Open http://127.0.0.1:8000/owned/ for Courtyard or http://127.0.0.1:8000/owned-orbit/ for Orbit Breaker. The catalog in examples/catalog.json drives browser staging, the documentation gallery and size reports. Both hosts are original JavaScript using browser APIs directly. No external runtime libraries, fonts, art downloads or CDN loaders are used.

## Native simulation and tools

The native examples currently run headless and export PNGs through the original software renderer:

```sh
cargo run --release -p owned-lab -- --frames 640 --output target/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --frames 1800 --screenshot target/orbit.png
cargo run --release -p ferravex-bench -- 4096
cargo run --release -p ferravex-fps --bin ferravex-server -- --bind 127.0.0.1:4000
```

The UDP server is for trusted development/LAN experiments. Courtyard has a separate browser WebRTC host/guest adapter with manual offer/answer pairing and no relay or account service. Browser and native network transports do not interoperate automatically. See the [netcode API](crates/ferravex-fps/README.md).

## Use the engine

```toml
[dependencies]
ferravex = { git = "https://github.com/nearbycoder/ferravex" }
```

The facade exposes core timing/entities and the engine's math, ECS, dynamics, graphics, gameplay, input, audio, assets, profiling, replay and networking APIs. There are no optional external renderer or physics features.

## Documentation and validation

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo build --release --workspace --bins --offline --locked
python3 scripts/smoke-games.py --output target/game-smoke
node examples/owned-orbit/tests/platform-input.mjs
python3 scripts/build-site.py
python3 -B scripts/test_audit_owned.py
python3 -B scripts/test_site.py
python3 scripts/serve-site.py --port 8080
```

The site contains guides, first-party Rust API docs and the two playable examples. GitHub pushes to main deploy its Dockerfile on Railway. Generated output lives in dist and is not committed.

This is an engine foundation. Native window/GPU/audio hosts, rotational rigid-body physics, secure native internet networking and device validation remain substantial work. The [implementation map](docs/FROM_SCRATCH.md) describes the twenty feature tracks and their limits; [platform status](docs/PLATFORMS.md) distinguishes compilation from runtime support. Adapter-dependent examples and their assets have been removed.

## License

MIT. All engine/game source and authored assets are first-party. Build toolchains, operating systems and browser implementations have their own licenses.
