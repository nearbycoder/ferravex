# Performance and size

Measure the first-party engine with reproducible workloads. Report the machine, toolchain, profile, workload and exclusions with every performance claim. The recovered renderer and rotational solver need fresh measurements; historical measurements of the previous backend are not results for this implementation.

## Simulation workloads

```sh
cargo run --release -p ferravex-bench -- 4096
cargo run --release -p ferravex-dynamics --example benchmark
```

The workspace benchmark warms each workload for 256 ticks, then measures the requested tick count with `std::time::Instant`. It reports mean, p50 and p99 for sixteen-player shooter simulation and 256-body scenes in two and three dimensions. These physics scenes use `ferravex-dynamics`, the translational solver; they do not benchmark the new `ferravex-physics` rotational solver. Rendering, networking, asset startup and GPU work are excluded. The scenes are regression tools, not predictions of complete game cost on another device.

## Browser and native artifacts

```sh
python3 scripts/build-web.py release
python3 scripts/report-size.py --profile release
cargo build --release --workspace --bins --offline --locked
```

The local catalog has fifteen examples. Size reporting should include each compiled game module plus its required host files; the thirteen restored examples also share `web/runtime.js`. Courtyard and Orbit Breaker retain their separate hosts. Report actual uncompressed bytes and deterministic gzip bytes separately. Documentation HTML, screenshots and precompressed copies are site payloads, not the engine's executable size, memory footprint or complete disk footprint.

The default release profile favors throughput. The `small` profile favors size and requires its own throughput measurements. Builders recreate generated output to avoid stale binaries and loaders; building locally does not deploy it.

## Render measurements

The immediate renderer records transformed clip-space triangles and vertex colors in Rust. GPU hosts submit batches through WebGL2 in the browser and OpenGL/GLX on Linux. Mesh construction can be cached, but transformed vertices are still emitted each frame. Measure CPU transformation/lighting, generated vertices, draw batches, upload bytes and GPU frame time separately before choosing further optimizations. Built-in text is triangle geometry from bitmap glyphs. The retained path added in iteration 2 keeps immutable mesh geometry on the GPU and streams instance transforms; immediate primitives/text still use the CPU path. Frustum/distance culling and hysteretic LOD are shared; Low/Medium/High lighting selects bounded local lights and directional shadow cost.

Courtyard retains its own persistent mesh/instancing renderer and scene tools for CPU mean/p99, visible instances, draw calls and asynchronous GPU timers where the browser supports them. Its timings do not automatically characterize `ferravex-render`. CPU frame timing includes JavaScript submission and Rust simulation; GPU timing excludes DOM/audio work.

Orbit renders into an original Rust RGBA framebuffer presented through Canvas2D. Courtyard and Orbit's native PNG paths include software rendering and file writes. Restored-game screenshots read back a native GPU frame. Neither screenshot path is an interactive frame-rate benchmark.

## Verification

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
python3 scripts/smoke-games.py --output target/game-smoke
```

The full smoke run requires Linux X11/GLX and a display. On a headless machine, select `--game owned-lab --game owned-orbit` or provide a suitable display/graphics environment before running the restored games. The smoke report records bounded execution and PNG output, not performance certification. Recovery-wide validation is pending; consult actual local build/test reports instead of inferring passes from the commands above.

## Local performance regression checks

Build and record all fifteen catalog examples on the current machine:

```sh
python3 scripts/perf-games.py --write-baseline target/perf/baseline.json
python3 scripts/perf-games.py --baseline target/perf/baseline.json
```

Each run uses fixed simulation intervals and the games' existing autoplay inputs
(Courtyard uses its fixed idle/training workload). Three fresh processes each
discard 60 warmup frames and record 180 measured frames. Select a subset with
repeated `--game` arguments. Linux interactive examples need DISPLAY and OpenGL;
Courtyard and Orbit Breaker use their software renderers. Platform, renderer,
dimensions, GPU timer source, render quality and workload configuration must match the baseline. Use `--quality low|medium|high` for explicit shared-renderer overrides.

`target/perf/current` contains raw per-frame JSON and a summarized `report.json`.
Metrics include CPU update/geometry preparation, CPU driver submission/swap,
software rendering where applicable, actual draw calls/vertices including directional shadow passes,
upload bytes, successful Rust allocation/reallocation calls and requested bytes,
and rigid-physics step/pair/contact/CCD counts. Allocations exclude C/driver
allocations and do not measure live heap size. Rigid solver counters sum all
catch-up steps. No rigid solver calls means zero work, not an inferred timing.
Unsupported counters are null. Schema 2 records asynchronous GPU execution queries
on the shared native host with source and per-metric sample count/coverage. GPU
results are tagged to their actual frame; missing results remain null. Submission
wall time can include driver/swap blocking and is separate from GPU execution.
The query surrounds clear/shadow/main drawing, excluding uploads and swap. A final
finish drains outstanding results outside the measured interval. Software hosts
retain null GPU results. Browser `?profile` exposes disjoint-aware GPU results;
see [lighting](LIGHTING.md).

Native reporting is opt-in through `FERRAVEX_PERF_OUTPUT`; normal play does not
allocate a recording buffer. The game entry macro installs a forwarding System
allocator; counting is enabled only for recordings. Custom hosts can install
`ferravex_core::telemetry::TrackingAllocator` once and use `Session`, `FrameToken`
and `FrameSample`. A recording retains at most 8192 measured frames and reports
overflow; the runner rejects incomplete output. Screenshot encoding, setup and
report serialization are outside measured frames. Presentation pacing is disabled
for interactive recordings, but drivers can still impose blocking.

The runner compares mean/p95/p99: timing budgets allow the greater of 20% or
200 microseconds of CPU noise (GPU query noise has a separate 20-microsecond floor); work budgets allow the greater of 5% or one unit.
Native/available Wasm raw and gzip artifact budgets allow the greater of 5% or
1024 bytes growth. Tolerances are explicit CLI options. GPU query coverage below 50% or a drop of over 10 percentage points fails. Other
partial metrics fail. Missing metrics, changed artifact coverage and incompatible machines fail comparison. Wasm size is only
included when an artifact exists; rebuild it before claiming current Web sizes.
Use `--no-build` only when taking responsibility for executable freshness.

Record baselines deliberately on an otherwise idle machine. Do not automatically
replace a baseline following a failure. These are local regression measurements,
not portable frame-rate promises. Run `python3 -B scripts/test_perf.py` for the
comparison/coverage regressions, plus the relevant Rust tests and native game
smoke checks before merging. iOS and other unavailable devices are not required
for this local iteration series.

The first local comparison on this shared Linux host flagged sub-millisecond
tail variation in unchanged binaries. Use a measured host-specific timing floor
(e.g. `--time-floor-us 1000` here) when reproducing those results; the stricter
default remains useful on quiet dedicated machines. Keep the chosen budget with
the comparison result rather than claiming every timing fluctuation is a code
regression.

### Retained rendering iteration

On the same Linux x86_64 host, the fixed 120-frame workload (30 warmup, two
fresh processes) produced the following local means, in microseconds. The browser
verification tab was closed before the final recording. These are CPU wall-time
measurements; they are not GPU execution times or portable frame-rate promises.

| Game | Previous CPU preparation | Retained CPU preparation | Previous submission | Retained submission |
|---|---:|---:|---:|---:|
| Strike | 154.1 | 7.0 | 46.9 | 61.9 |
| Dune Skimmer | 311.4 | 121.1 | 59.4 | 89.7 |
| Sky Courier | 310.6 | 121.4 | 54.1 | 95.6 |
| Marble Run | 177.9 | 72.4 | 54.5 | 114.0 |
| Crystal Cavern | 258.8 | 118.7 | 53.0 | 224.3 |

Strike's mean uploads fell from 421,152 to 34,480 bytes/frame. Crystal Cavern
illustrates the remaining tradeoff: many small unique meshes increase draw and
submission work, offsetting CPU preparation savings. Retained buffers are a
foundation, not a claim that every example is already faster. Visibility/LOD is
next, and final iteration validation must revisit total CPU, GPU where available,
and artifact budgets. Raw reports remain in `target/perf/iteration-02-final`.

### Visibility iteration

The same fixed local workload after frustum culling reduced Crystal Cavern's
mean retained/immediate draw commands from 229 to 118.2, submitted vertices from
33,180 to 26,025.3, and submission CPU time from 224.3 to 147.8 microseconds.
Dune Skimmer submitted vertices fell from 37,532.5 to 28,856.5. Other games can
show small-scene test overhead or host timing noise; use the raw workload and
full-frame measurements before making broader performance claims. The per-frame
object visibility counters are also available through the runtime API and browser
canvas attributes. Raw recordings are in `target/perf/iteration-03`.


### Shared lighting iteration

Native release recordings on Linux x86_64 with AMD Radeon 8060S (Mesa 26.2.4),
120 measured frames after 30 warmup frames in each of two fresh processes, gave
these GPU draw-time means in microseconds. Browser verification tabs were closed.

| Game | Low | Medium (game default) | High |
|---|---:|---:|---:|
| Strike | 36.3 | 55.2 | 66.8 |
| Dune Skimmer | 41.2 | 55.6 | 64.5 |
| Crystal Cavern | 53.0 | 59.8 | 59.8 |

GPU query coverage was 100% for the thirteen shared-host native examples; the two
software hosts correctly had no GPU duration. Crystal's Medium/High light cap
covers its three local lights and it has no sun/shadow pass. Strike's mean calls
rose from 4 on Low to 9 on Medium/High; Dune's rose from 23 to 60. These costs
include extra off-camera casters, and exclude uploads/swap. CPU preparation and
submission remain separate metrics. A same-configuration repeat of all fifteen
examples passed using a 1000-microsecond CPU host noise floor and the separate
20-microsecond GPU floor. This is a reproducibility check, not a speedup claim.
Schema-1 recordings cannot be compared to the new GPU/quality-aware schema.
Raw reports are in ignored `target/perf/iteration-05-{default,low,high,repeat}`.


### Character-controller iteration

With the same native release workload/machine as iteration 5 (120 measured frames,
30 warmup, two processes), CPU update means were 17.2 µs for Strike (previously
7.2), 20.8 µs for Courtyard (11.5), 67.2 µs for Emberline (80.0) and 96.8 µs for
Crystal Cavern (105.2). The two shooter paths now run continuous capsule queries.
Small decreases are host/workload observations, not a portable speedup claim.
The motor uses fixed stack scratch arrays. Reusing Strike's remote-player vector
reduced its mean per-frame allocation bytes from 474.3 to 192.7 despite the larger
player state; it no longer allocates a fresh list each simulation update.

The shared motor has a visible size cost in release browser modules:

| Game | Previous Wasm bytes | Controller Wasm bytes | Controller gzip bytes |
|---|---:|---:|---:|
| Courtyard | 238397 | 259110 | 97146 |
| Emberline | 141628 | 164810 | 63825 |
| Crystal Cavern | 212790 | 233634 | 89833 |
| Strike | 227653 | 249113 | 96258 |

This adds roughly 21–23 KB raw / 8–10 KB gzip per affected module. The comparison
passed CPU/GPU timing and work budgets with the recorded 1000 µs CPU / 20 µs GPU
floors, but **failed the default 5% artifact-growth gate** for these new features
(including Crystal's native binary and Emberline's compressed native binary).
These flags remain in the report; the previous baseline was not replaced or the
size tolerance widened. They are an explicit cost for the final iteration-20
size/performance audit. An inlining experiment increased size and was discarded.
Raw results: `target/perf/iteration-06-final/report.json`.


### Persistent-contact iteration

The 128-box rigid benchmark on this Linux/Radeon host (native release, 1200 ticks
at 120 Hz) reported these CPU observations with the new solver:

| Configuration | Mean µs/tick | Last 120 ticks, mean µs | Final sleeping bodies |
|---|---:|---:|---:|
| Warm start, sleep enabled (default) | 251.9 | 132.3 | 128 |
| Warm start, sleep disabled | 1711.7 | 1886.0 | 0 |
| Cold start, sleep disabled | 2634.4 | 3013.6 | 0 |

All three use twelve velocity iterations. Evolving contact sets also affect
these timings: the cold case merges its stacks into one island, while the warm
cases retain 32. An earlier observation with the previous solver/default of 24
velocity iterations averaged 492 µs/tick; both the implementation and default
iteration count changed, so this is an end-to-end observation rather than an
isolated warm-start speedup. Raw benchmark logs: `target/iteration-07-bench-*`.
A separate same-iteration twelve-box test stays upright at eight velocity/eight
position iterations with warm starting and collapses in the cold comparison.
Four velocity iterations still retain appreciable residual motion in this stress
case; they are not advertised as default-quality simulation.

The same fifteen-game fixed workload as iteration 6 recorded these mean Rust
allocation calls per frame:

| Game | Previous calls | New calls | Previous CPU µs | New CPU µs |
|---|---:|---:|---:|---:|
| Pinball | 9.47 | 1.06 | 113.1 | 137.5 |
| Marble Run | 21.00 | 3.00 | 105.2 | 99.1 |
| Siege Foundry | 32.04 | 4.06 | 215.0 | 185.8 |
| 2D Physics Lab | 47.52 | 3.00 | 396.0 | 353.3 |
| 3D Physics Lab | 45.73 | 3.00 | 602.2 | 388.9 |

These include game/HUD work. A dedicated steady-contact allocation test isolates
the solver and verifies zero allocations after warmup, awake and asleep. Timing
values are local observations with shared-host noise, not uniform improvements
or portable frame-rate guarantees.

The default comparison retained three flags: Pinball raw Wasm grew from 222853 to
235050 bytes, the 2D lab from 211734 to 222577 bytes, and the 3D lab's contact-point
p99 was 276 versus a 275.1 budget (baseline 262). Compressed size growth stayed
within the default gate; the contact count reflects a changed solver workload.
CPU/GPU timing and the other work comparisons passed the recorded 1000 µs CPU /
20 µs GPU floors. Neither baselines nor tolerances were changed to hide flags.
Raw report: `target/perf/iteration-07-final/report.json`. Carry these and iteration
6's artifact costs into the final size/performance audit.
