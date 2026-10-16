# Example validation

The local catalog contains fifteen examples built from first-party crates and original browser hosts. Thirteen games/labs were restored and ported to a new renderer and rotational physics implementation. Their previous backend results do not validate this recovery. The results below were verified locally against this recovery; compilation and short captures remain distinct from full device or production validation.

## Restored games and labs

| Example | Gameplay and integration exercises |
| --- | --- |
| Emberline | Platform collision, buffered/coyote jumps, hazards, checkpoints and complete traversal |
| Neon Swarm | ECS queries, projectile/cover sweeps, waves, health, dash and restart |
| Pocket Rally | Ordered forward gates, off-road drag, race finish and session ghost preservation |
| Salvage Pinball | Launch gating, contact scoring, drains and rotating flipper momentum |
| Orchard Guard | Purchases/upgrades, targeting, slows, wave progression and entity bounds |
| Marble Run | Sphere settling/rotation, braking, pickups, checkpoints and course completion |
| Dune Skimmer | Gate apertures/order, obstacle and sand response, boost and recovery |
| Crystal Cavern | Swept walls, interaction visibility, puzzle prerequisites and exit progression |
| Siege Foundry | Stable stacks, ballistic contacts, ammunition/cooldown and target demolition |
| Sky Courier | Ordered rings, speed/altitude delivery, collisions, timeout and restart |
| Strike | Shared shooter simulation, local input/rendering and native UDP client integration |
| 2D Physics Lab | Body spawning, contacts, reset/pause and visible rotational behavior |
| 3D Physics Lab | Body stacks, camera controls, impulses and visible rotational behavior |

Headless game tests exercise rules and simulation. Native and browser runs additionally need visual/input checks: correct camera orientation, low-poly geometry, legible HUD/text, resize behavior, controls, failure/restart and victory where applicable. Autoplay should use ordinary game inputs rather than teleporting or directly awarding progress. Physics-dependent results must be rechecked after solver changes.

## Courtyard and Orbit Breaker

Courtyard shares shooter simulation with the native authority. Tests cover movement, collision, hitscan occlusion, weapons/reload, life cycles, prediction/replay, UDP sessions, reliable event delivery, malformed packets and historical rewind. Its headless smoke path advances simulation, software-renders a frame and exports a PNG. The separate browser host needs interaction and WebRTC pairing checks.

Orbit Breaker uses continuous ball queries, particles, sprite clips and buffered input. Tests cover high-speed collision, victory through ordinary autoplay controls, loss/restart, pickup expiry, input edges and opaque frame output. The JavaScript regression checks short key taps, direction holds and blur-to-pause.

## Run local checks

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
node examples/owned-orbit/tests/platform-input.mjs
node crates/ferravex-render/tests/browser-input.mjs
python3 scripts/test-strike-local.py
cargo build --release --workspace --bins --offline --locked
python3 scripts/smoke-games.py --frames 180 --output target/game-smoke
python3 scripts/build-site.py
python3 -B scripts/test_site.py
python3 -B scripts/test_audit_owned.py
```

The full smoke runner requires Linux X11/GLX and a working display for restored games. It checks fresh PNG output, checksums, scanline layout, nonblank sampled colors, dimensions, successful exit status and bounded execution. Use `--game owned-lab --game owned-orbit` for the two headless hosts. A short smoke capture establishes startup/rendering only, not every game's win condition, complete input support or frame-rate target. Results are written to `target/game-smoke/smoke-results.json` by default.

The catalog drives browser staging and site checks. Validate all fifteen expected modules, shared runtime references, links and generated page behavior against the current catalog; reject stale adapter artifacts. No push, deployment or remote CI run is part of this recovery. Browser interaction and physical-device testing remain distinct from compilation and native screenshots.

## Local recovery results, 2026-10-15

- All 28 Cargo packages are workspace-owned across every feature and target; zero external Cargo packages.
- `cargo test --workspace --all-targets --all-features --offline --locked`: **242 passed, zero failed**, one display-dependent GPU test excluded by default. That GPU depth/readback test also passed when explicitly run on this machine's AMD GPU.
- Four Rust documentation tests passed. Strict Clippy with Rust 1.99 and whole-workspace/all-target WebAssembly checks passed. Formatting and dependency-policy tests passed.
- All fifteen native examples completed fresh 360-frame runs with valid, nonblank PNG captures. The five physics games/labs were rebuilt and rerun after the final solver CCD fix. Reports: `target/recovery-smoke/smoke-results.json` and `target/recovery-physics-final/smoke-results.json`.
- All thirteen restored browser examples rendered advancing frames with no console errors in the local WebGL2 host. Strike's menu and play HUD were checked interactively; the retained Courtyard and Orbit Breaker hosts also entered play without browser errors. Browser input regressions cover short taps, logical-pixel DPI, cancellation, focus reset, pointer-lock Escape and FPS touch separation.
- The real Strike native client joined an ephemeral localhost UDP authority and received advancing snapshots (`connected=true`, `server_tick=96` in this run). Report: `target/recovery-net/result.json`.
- The local site builds nineteen guides and fifteen playable examples. Its six server/catalog/link tests and five dependency-audit regressions passed.

The captures and reports are local build artifacts. Gallery previews are refreshed from those original-host captures using only Python's standard-library PNG compression. No GitHub push, CI trigger or deployment was performed. Native Windows/macOS/iOS window hosts, physical mobile testing and unrestricted production physics/networking remain outside this verified result.


## Platform iteration checks (2026-10-16)

The native/browser host iteration adds Windows/macOS Rust cross-target checks,
first-party platform audio sinks, logical/framebuffer DPI separation and context
reconstruction. Available Linux checks exercise 2× DPI readback, recovered
retained skins/shadows, recovered last-valid shaders after a failed edit, timer
frame-tag continuity and audio suspension/resume without capture truncation.
Real browser interaction exercised context loss/restoration in Strike and
Crystal Cavern, including a rejected custom material revision; an actual-Wasm
contract verifies preserved shader descriptors. All fifteen native/Wasm builds
and native gameplay/PNG smoke checks remain in the local validation set.
Windows/Apple runtime and SDK linking were unavailable. iOS SDK/simulator/device
checks remain excluded. See [Platforms](PLATFORMS.md) and [Performance](PERFORMANCE.md)
for exact scope and measured costs. CI stays enabled in the repository; this
iteration uses `[skip ci]` commits and local checks.
