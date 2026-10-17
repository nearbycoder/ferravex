# Ferravex

An original Rust foundation for compact 2D and 3D games. The workspace uses only first-party crates and Rust std: math, ECS, physics, rendering, gameplay, input, audio, assets, replay and netcode are implemented here. Browser and operating-system APIs form the platform boundary. See the [dependency policy](DEPENDENCIES.md).

Build from this checkout for the current game catalog and browser player tools. Website publication is separate from merging source changes; a deployed site may show an earlier revision. Platform validation limits are documented in [Platforms](docs/PLATFORMS.md).

## Play and build locally

The [catalog](examples/catalog.json) contains fifteen examples: the ten original 2D/3D games, Strike, two physics labs, Courtyard and Orbit Breaker. The thirteen restored examples use the original `ferravex-render` runtime. Courtyard retains its instanced WebGL2 renderer; Orbit Breaker presents its Rust framebuffer through Canvas2D.

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
rustup target add wasm32-unknown-unknown
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Open http://127.0.0.1:8000/ for the local catalog. Individual pages include `/emberline/`, `/neon-swarm/`, `/strike/`, `/owned/` for Courtyard and `/owned-orbit/` for Orbit Breaker. Browser hosts use standard APIs directly, with no external runtime libraries, fonts, art downloads or CDN loaders.

## Native games and tools

The restored games use first-party native/window/GPU bindings. Linux uses X11/GLX and needs a working display and OpenGL 3.3 driver; Windows and Apple host source and its verification status are described in [Platforms](docs/PLATFORMS.md):

```sh
cargo run --release -p neon-swarm
cargo run --release -p marble-run
cargo run --release -p strike
```

Courtyard and Orbit Breaker retain their native headless simulation and PNG export paths:

```sh
cargo run --release -p owned-lab -- --frames 640 --output target/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --frames 1800 --screenshot target/orbit.png
cargo run --release -p ferravex-bench -- 4096
cargo run --release -p ferravex-fps --bin ferravex-server -- --bind 127.0.0.1:4000
```

The UDP server is for trusted development/LAN experiments. Courtyard has a separate browser WebRTC host/guest adapter with manual offer/answer pairing and no relay or account service. Browser and native network transports do not interoperate automatically. See the [netcode API](crates/ferravex-fps/README.md).

## Use this checkout

```toml
[dependencies]
ferravex = { path = "../ferravex/crates/ferravex" }
ferravex-render = { path = "../ferravex/crates/ferravex-render" }
```

The facade exposes timing, entities, math, ECS, translational dynamics, software graphics, gameplay, input, audio, assets, profiling, replay and networking. The restored examples depend directly on the immediate renderer and, where needed, `ferravex-physics` for rotational rigid bodies. There are no optional external renderer or physics backends.

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

The full native smoke run needs a Linux X display for the restored games. Use `--game owned-lab --game owned-orbit` for only the headless hosts. Generated web/site output lives in `dist`; these commands build and serve it locally.

This is an engine foundation with original native/browser hosts, shared GPU rendering, audio and development/LAN multiplayer. Windows/macOS Rust cross-target checks pass; their runtime/SDK linking and the iOS source still need device validation. Internet services and a Metal backend remain future work. The [implementation map](docs/FROM_SCRATCH.md), [architecture](docs/ARCHITECTURE.md), [validation guide](docs/GAME_TESTS.md) and [platform status](docs/PLATFORMS.md) describe the implemented systems and their limits.

## License

MIT. Engine/game source and authored assets are first-party. Build toolchains, operating systems and browser implementations have their own licenses.

## Compose gameplay quickly

[Gameplay recipes](docs/GAMEPLAY_RECIPES.md) map reusable reward, weapon, encounter, interaction, platform and trial rules to their host contracts. Run `cargo run -p ferravex-play --example gameplay_recipe --offline --locked` for a small survival/crafting composition. The [40-feature series](docs/GAMEPLAY_40.md) applies the rules to Neon Swarm, Emberline and Orbit Breaker.

## The player update

All fifteen browser examples share the new player tools: frame limits, screenshots, clean sharing, wake lock, active session timing, accessibility preferences and settings backups. Read the [player guide](docs/PLAYER_GUIDE.md) for controls and browser limits. The [30-change catalog](website/releases/player30.json) links each improvement to its individual PR.
