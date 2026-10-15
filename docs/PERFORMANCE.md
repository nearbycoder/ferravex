# Performance baseline

## Owned engine checkpoint — 2026-10-15

The owned Courtyard build is 181,328 bytes of Wasm (68,274 bytes gzip); its complete
Wasm/HTML/JS/GLSL folder is 208,964 bytes before transfer compression. Orbit Breaker
is 107,583 bytes of Wasm (41,262 bytes gzip), 116,144 bytes for the complete folder.
The provided Python development server serves uncompressed files; gzip figures
measure potential transfer size, not configured server behavior. Neither example
loads third-party runtime packages or external assets. Different game content
makes a direct size comparison with the earlier Strike build inappropriate.

The original dynamics benchmark warms a scene of 513 bodies/512 resting contacts,
then measures 1200 steps. It reported 0.188 ms/step on this host during this run.
It excludes angular bodies (unsupported), rendering, networking and active pile
stress. Reproduce with `cargo run --release -p ferravex-dynamics --example benchmark`.
Courtyard's 640-tick headless run took 5.55 ms for simulation; its single 960×540
software render plus PNG encoding/file write took 31.49 ms. These are single-run
observations, not cross-device or competitive performance claims.

The browser inspector reports CPU mean/p99, draw calls/visible instances, and GPU
time through asynchronous WebGL timer queries when supported. CPU frame timing
includes JS submission and Rust simulation; GPU timing excludes DOM/audio work.
The browser test environment's GPU timings are not representative of a phone.
See [exact artifacts and validation](reports/owned-engine.json), the original
crate benchmark and the [remaining feature work](FROM_SCRATCH.md).


Recorded October 15, 2026 on Linux x86-64, AMD Ryzen AI Max+ 395 with Radeon 8060S, Rust 1.97.0. These are local development measurements, not cross-device or cross-engine comparisons. CPU clock/thermal state and scheduling were not controlled.

## Headless simulation

`cargo run --release -p ferravex-bench -- 4096` warms each workload for 256 ticks, then measures 4,096 ticks with `std::time::Instant`. Rendering, networking, asset startup, and GPU work are excluded. The FPS workload includes movement, firing, reloads, death and respawn; not every actor fires every tick. Physics sleep is disabled for the measured bodies. Timer and sampling overhead are included.

| Workload | Mean | p50 | p99 | Total |
| --- | ---: | ---: | ---: | ---: |
| 16-player FPS simulation | 1.48 µs | 1.33 µs | 5.36 µs | 0.006 s |
| 256 awake 2D bodies | 55.82 µs | 55.72 µs | 59.38 µs | 0.229 s |
| 256 awake 3D bodies | 132.33 µs | 131.29 µs | 140.98 µs | 0.542 s |

A 64 Hz simulation has 15.625 ms per tick; these simple scenes use a small fraction of that on this machine. This does not predict full-game p99 cost, large scenes, mobile thermals, rendering latency, or internet multiplayer performance. The benchmark is intended to expose regressions as the engine grows.

## Built artifact sizes

Exact file lengths and deterministic gzip level-9 estimates from `scripts/report-size.py`. Gzip is a transfer estimate; the included development server sends uncompressed files. Native executables also require the platform system libraries. Shader/geometry data and the default font are embedded. The loader and HTML are separate browser downloads.

### release

| Artifact | File bytes | Gzip bytes |
| --- | ---: | ---: |
| native/physics-2d | 2,432,296 | 1,127,573 |
| native/physics-3d | 2,971,912 | 1,385,423 |
| native/strike | 1,115,200 | 510,163 |
| native/ferravex-server | 429,968 | 209,607 |
| wasm/physics-2d.wasm | 1,972,105 | 705,896 |
| wasm/physics-3d.wasm | 2,454,536 | 895,818 |
| wasm/strike.wasm | 558,244 | 188,381 |
| web/mq_js_bundle.js | 37,427 | 10,340 |

### small

| Artifact | File bytes | Gzip bytes |
| --- | ---: | ---: |
| native/strike | 993,712 | 462,677 |
| native/ferravex-server | 366,680 | 184,216 |
| wasm/strike.wasm | 426,476 | 162,589 |
| web/mq_js_bundle.js | 37,427 | 10,340 |

Only the FPS client/server were built with `small` for this comparison. The physics sandbox entries above use `release`. The served `dist/web` uses `release`. Size optimization reduced the FPS binary size; throughput of `small` was not benchmarked, so the performance default remains `release`.

## Rendering and network scope

The FPS submits about 4,000 triangles in its default view. Optional F3 counters reflect submitted scene geometry, not GPU profiling. Local native and browser runs showed roughly 8.3 ms/frame while capped around 120 FPS; that is a smoke-test observation, not an uncapped rendering benchmark or a minimum-hardware target.

The current maximum 16-player authority snapshot is 696 bytes; up to six redundant input commands fit in 141 bytes. At one full snapshot per three 64 Hz ticks, that is about 14.8 kB/s of snapshot payload per client before UDP/IP headers. It is a protocol budget, not measured deployed bandwidth. Delta snapshots and quantization remain future optimization work.

## Verification

40 Rust tests including two doctests pass, covering physics contacts/CCD/rays, fixed timing, protocol rejection, movement, wall-occluded hitscan, prediction/replay, two-client UDP identity/damage, sustained packet loss, session renewal and slot reuse. Strict Clippy and formatting pass. Four loader-patch regressions pass. All examples build as native Linux binaries and WebAssembly, and all three browser demos render with clean console checks after input/shader/font initialization fixes. A graphical native client also completed a real dedicated-server connection smoke test.

Windows/macOS/iOS runtime and Metal shader validation remain unverified. See [platform status](PLATFORMS.md).
