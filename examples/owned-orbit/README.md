# Orbit Breaker

A playable original 2D paddle-and-ball game built exclusively with Ferravex's first-party crates and Rust std. Clear two fields of 35 destructible panels. Reinforced panels in sector two take two impacts. Three misses end the run; turquoise capsules widen the paddle for nine seconds. Paddle contact position controls the return angle.

- **A/D or arrow keys:** move
- **Mouse/touch drag:** steer in the browser
- **Space, Enter, or tap:** launch/retry
- **Escape:** pause in the browser
- **R:** restart

The simulation runs at 120 Hz with Ferravex's `FixedClock`. Every ball displacement uses `ferravex-dynamics::PhysicsWorld<2>::cast_ball` for continuous circle/box collision against the paddle, walls, and panels; the game applies its arcade reflection rule and bounds consecutive impacts. This exercises the engine's original CCD queries rather than a third-party physics library. Particles use `ferravex-play::ParticlePool`, capsule pulses use `SpriteClip`, and press/release edges pass through `ferravex-engine::ActionMap`.

All visuals—including the original bitmap lettering—are rasterized into an RGBA framebuffer in Rust. The browser presents this with `CanvasRenderingContext2D.putImageData`; it contains no game rules, rendering library, or JavaScript dependency. The CPU raster is a portable 2D reference path, not a GPU renderer. No external assets or audio are included.

```sh
cargo test -p owned-orbit
cargo clippy -p owned-orbit --all-targets -- -D warnings
node examples/owned-orbit/tests/platform-input.mjs
cargo run --release -p owned-orbit -- --autoplay --frames 1800 --screenshot /tmp/orbit.png
cargo build --release --target wasm32-unknown-unknown -p owned-orbit --lib
```

The native CLI is a deterministic headless runner and PNG exporter. Frame counts represent 120 Hz simulation steps. Interactive play is in the browser host at `/owned-orbit/`.

To stage the browser build, copy `target/wasm32-unknown-unknown/release/owned_orbit.wasm` alongside `web/owned-orbit/index.html` and `platform.js`, preserving the filename, and serve that directory over HTTP. The workspace build script can perform this copy.

The WebAssembly ABI exports `init()`, `step(dt: f64, axis: f32, buttons: u32)`, `pixels()`, `width()`, `height()`, `status()`, `score()`, `lives()`, `sector()`, and `paddle()`. Button bit 0 is launch/retry; bit 1 is reset. RGBA bytes remain owned by the module and are read immediately after `step`. Status codes: 0 ready, 1 playing, 2 sector transition, 3 victory, 4 loss.

Regression tests verify high-speed CCD (12,000 pixels/second), full two-sector victory through ordinary autoplay controls, loss/restart, pickup expiry, short input taps retained across frames without simulation ticks, and complete opaque RGBA output.

Browser verification exercised Space launch, pointer steering, pause/resume, and Restart with no console errors. A host regression now latches very short launch/restart taps between animation frames; its Node test also checks held direction and blur-to-pause. The release Wasm measured 107,583 bytes (41,262 bytes with gzip) for this build. A deterministic full autoplay cleared all 70 panels in 177.36 simulated seconds with three lives remaining.
