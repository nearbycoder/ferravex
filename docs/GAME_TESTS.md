# Example validation

The shipped catalog contains two games, both built only from first-party Rust crates and original browser hosts.

## Courtyard

The 3D training ground shares the shooter simulation with the native authority. Rust tests cover fixed movement, collision, hitscan occlusion, weapons/reload, life cycles, prediction/replay, UDP sessions, reliable event loss/order/duplicates, malformed packets and historical rewind. The headless smoke path advances its simulation, rasterizes a frame and exports an original PNG.

## Orbit Breaker

The 2D game uses the original continuous ball queries, particles, sprite clips and buffered input. Tests cover high-speed collision, victory through ordinary autoplay controls, loss/restart, pickup expiry, input edges and opaque frame output. The JavaScript host regression checks short key taps, direction holds and blur-to-pause.

## Run checks

```sh
cargo test --workspace --all-features --offline --locked
node examples/owned-orbit/tests/platform-input.mjs
cargo build --release --workspace --bins --offline --locked
python3 scripts/smoke-games.py --frames 180 --output target/game-smoke
python3 scripts/build-site.py
python3 -B scripts/test_site.py
python3 -B scripts/test_audit_owned.py
```

The smoke runner requires fresh valid PNGs from both native hosts, with minimum image dimensions, successful exit status and bounded execution time. It runs without a display. It does not establish browser input, native GPU performance or mobile device support.

The site regression checks generated links, the exact two-game catalog, WebAssembly count, absence of retired artifacts, HTTP health, gzip, conditional requests, byte ranges and directory/path handling. CI compiles all workspace targets and checks native Linux, macOS, Windows, WebAssembly and both iOS SDK targets. Browser interaction must still be exercised after a host or presentation change.
