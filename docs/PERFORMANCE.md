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

Opt-in [post-processing](VISUALS.md#post-processing) is off by default and then
costs nothing. When enabled it adds 8 (Medium) or 10 (High) full-screen-triangle
passes: about 0.17–0.2 ms of extra native GPU time and 0.25–0.3 ms in Chrome
WebGL2 at 1280×800 on the measured Radeon 8060S, with no per-frame allocations.
Compare presets without editing a game:

```sh
FERRAVEX_DPI_SCALE=1 FERRAVEX_QUALITY=medium FERRAVEX_POST=neon \
  FERRAVEX_PERF_OUTPUT=target/post-neon.json \
  cargo run -p neon-swarm --release -- --autoplay --smoke-frames 900
```

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


### Joints and compound-body iteration

On the same Linux/Radeon native release workload as iteration 7 (120 measured
frames after 30 warmup frames in each of two processes), the expanded examples
and mass/joint implementation have these measured costs:

| Game | Previous Wasm bytes | New Wasm bytes | New gzip bytes | CPU mean µs |
|---|---:|---:|---:|---:|
| salvage-pinball | 235050 | 278475 | 106396 | 151.2 |
| marble-run | 271839 | 286067 | 109744 | 92.5 |
| siege-foundry | 262213 | 280659 | 108562 | 190.0 |
| physics-2d | 222577 | 273746 | 104883 | 544.8 |
| physics-3d | 260510 | 313205 | 119267 | 539.1 |

Pinball now drives dynamic capsule flippers with hinge motors. Both labs retain
the original falling bodies and add four linked bodies, three anchors, compound
parts and four connections. These scenes are therefore larger workloads than
the previous baseline. Mean 3D lab draws are 13 versus 11 before the mechanisms;
mesh grouping reduced an intermediate 20-draw version to 13. GPU mean is 31.0 µs
versus 28.1 before; 2D lab CPU mean is 544.8 µs versus 353.3, and 3D lab 539.1 µs
versus 388.9. These are machine/workload observations, not portable speed claims.
Idle hinge motors no longer force maximum-speed substeps, and collision layers
exclude unsupported flipper/fixture pairs. Pinball mean broadphase candidates are
1.33/frame (previous 0.54); its p95/p99 both reach 6 during motor activity.

Joint row implementations are installed when a world first inserts a joint.
Linkers can discard them for games without connections: this removed about 22 KB
raw / 8.5 KB gzip from an intermediate Marble Run/Siege build. Compound mass and
query support still have a cost even in those existing games. All five affected
Wasm modules exceed the original default 5% size-growth gate. Native/gzip size
flags also remain for Pinball and both labs.

The final comparison retains **46 flags** across size, lab geometry/upload/draw
counts, broadphase/contact counts, lab allocation bytes and Pinball candidate
quantiles. Both lab recordings include intermittent allocation growth inside the measured
window: mean allocation bytes/frame are about 120 versus 17 previously, with
p99 about 6161–6162 bytes, though calls stay around 3.02 versus 3. Dedicated
steady-state solver tests verify zero heap allocations after warmup for contact
stacks and continuously motor-driven compound mechanisms. That isolated result
does not erase allocation growth observed by the complete game recording.

CPU/GPU timing comparisons passed the recorded 1000 µs CPU / 20 µs GPU floors;
other work and artifact flags remain visible. Baselines and tolerances were not
changed. Raw results: `target/perf/iteration-08-final/report.json` (all fifteen
current release binary hashes verified). Carry these flags, plus iteration 6
and 7's costs, into the final iteration-20 audit.


### Shared-camera iteration

The same Linux/Radeon release workload as iteration 8 (120 measured frames,
30 warmup frames, two fresh processes) gives these observations for the seven
camera integrations:

| Game | Previous Wasm bytes | New Wasm bytes | New gzip bytes | CPU mean µs |
|---|---:|---:|---:|---:|
| emberline | 164810 | 168355 | 65047 | 73.4 |
| dune-skimmer | 204640 | 221081 | 86077 | 127.6 |
| marble-run | 286067 | 298660 | 114956 | 95.7 |
| sky-courier | 187586 | 199723 | 77178 | 123.4 |
| strike | 249113 | 260333 | 100838 | 17.6 |
| crystal-cavern | 233634 | 244537 | 94236 | 99.5 |
| physics-3d | 313205 | 324181 | 123928 | 616.5 |

All fifteen recordings passed CPU/GPU timing and work comparisons with the
recorded 1000 µs CPU / 20 µs GPU host floors. The comparison retains **four size
flags**: Dune Skimmer's raw/gzip Wasm and Sky Courier's raw/gzip Wasm exceed the
default 5% growth gate. Native and other artifact comparisons passed. The prior
baseline and tolerances were preserved; these flags join the earlier costs for
the final iteration-20 audit. Raw report:
`target/perf/iteration-09-final/report.json` (current native binary hashes checked).

Dune's whole-update CPU mean was 127.6 µs versus 88.5 before, and Marble's 95.7
versus 92.5. These include scenery queries, changed views/culling and game/HUD
work, not an isolated camera benchmark. Other observed decreases are shared-host
measurements, not portable speedup claims. A dedicated allocator test exercises
all modes, live obstruction sweeps and layered shake with **zero heap allocations**,
including initialization. Dune builds its static authored blocker vector once;
Marble's adapter reads its current rigid colliders without a copied query scene.

Local gameplay checks completed Dune's three laps with zero impacts/recoveries,
Marble's six-crystal course with zero falls, and Sky Courier's six-beacon delivery
with full hull. Browser checks also completed Emberline with zero falls and
Crystal Cavern's three-crystal escape, checked Strike practice aiming, and
exercised the lab's drag-to-orbit controls. These are local functional checks,
not device-wide performance or collision guarantees.

The camera guide also exposed a search coverage gap: the indexer now retains
real facade-owned APIs and methods without Rustdoc detail wrappers, while
excluding re-export mirrors and redirect stubs. The current corpus is 2719
entries, roughly 1.4 MB raw JSON / 190 KB gzip, loaded when search opens. Local
retrieval checks averaged about 0.7 ms/query; the extra searchable API content
has a transfer/storage cost even though it is separate from game modules.

## Iteration 10: shared player input

All fifteen browser games now use the shared action core and controls panel;
the thirteen shared-renderer games also use it on Linux. The two owned native
programs remain headless reference runners. Contexts, buffered press/release and
motion, captured touch roles, selected-controller response, validated saved
profiles and rebinding are implemented in first-party Rust. Browser/OS event
bindings and storage are the host boundary; see [input](INPUT.md).

Local validation included the workspace suite, strict native and Wasm Clippy,
release native/Wasm builds, all fifteen native gameplay smoke runs, browser API
contract tests and offline Wasm input/panel checks for all fifteen modules. Live
browser checks covered rebinding, persistence after reload, gameplay with a
rebound key, custom Canvas/WebGL overlays, and a 390×844 controls layout. A real
browser test caught and fixed Base64 helper invocation context during saving.
Core feeds/context changes/steady touch layout pass the zero-allocation test;
the open controls editor can allocate text/layout data. No physical controller
was attached, so Linux/browser device hardware behavior is not certified. Other
native platform implementation remains iteration 20; no iOS SDK/device tests ran.

The final report is `target/perf/iteration-10-final/report.json`, compared with
iteration 9 using two runs, 30 warmup and 120 measured frames per run. Linux
x86_64, the same release profile/default quality and autoplay workload, 20% time
allowance, 1,000 µs CPU/submission floor, 20 µs GPU floor and 5% artifact size
allowance were retained. Every timing and work gate passed. **56 size flags
remain:** raw/gzip Wasm for all fifteen games and raw/gzip native files for the
thirteen interactive native games. Editor/profile functionality is currently
linked into each game; these costs need attention during the final size audit.
The baseline and tolerances were not changed to hide them.

| Game | Wasm bytes | Gzip-9 bytes | Raw increase vs 9 | Gzip increase vs 9 |
|---|---:|---:|---:|---:|
| owned-lab | 340,433 | 127,249 | +81,323 | +30,074 |
| owned-orbit | 224,678 | 84,728 | +58,524 | +22,499 |
| emberline | 232,159 | 88,428 | +63,804 | +23,381 |
| neon-swarm | 247,534 | 91,739 | +65,382 | +24,052 |
| pocket-rally | 219,596 | 82,898 | +65,854 | +24,704 |
| salvage-pinball | 343,342 | 130,320 | +64,867 | +23,926 |
| orchard-guard | 212,293 | 80,527 | +90,987 | +34,063 |
| marble-run | 365,091 | 139,743 | +66,431 | +24,787 |
| dune-skimmer | 288,284 | 111,102 | +67,203 | +25,025 |
| crystal-cavern | 315,448 | 120,557 | +70,911 | +26,321 |
| siege-foundry | 347,097 | 133,629 | +66,438 | +25,075 |
| sky-courier | 266,019 | 101,660 | +66,296 | +24,482 |
| strike | 326,664 | 125,295 | +66,331 | +24,457 |
| physics-2d | 337,973 | 128,764 | +64,227 | +23,877 |
| physics-3d | 389,861 | 148,438 | +65,680 | +24,510 |

An earlier measurement is preserved under
`target/perf/iteration-10-before-cursor-fix`; it had 56 size flags plus a 2D lab
GPU p99 flag (72.94 µs versus a 47.33 µs allowance). The final implementation
only draws a virtual pointer after stick/keyboard cursor input, avoiding a
redundant cursor over a real mouse. Final GPU timing passed; this is a whole
workload observation, not an isolated input optimization measurement.

Shared browser input/overlay JavaScript is reported separately by
`scripts/report-size.py`. The numbers above are actual game module bytes,
excluding documentation, screenshots, precompressed copies and container images.

## Iteration 11: shared runtime audio

The shared Rust mixer now supplies all fifteen browser examples and thirteen
interactive Linux examples; native Courtyard retains its headless PCM buffer.
The original bank supplies surface footsteps/impacts, weapon/reload/outcome cues
and managed motor loops. Validated sounds support handles, priority, pitch,
volume buses, positional falloff/panning and gain/stop ramps. Linux device access
uses original dynamically loaded ALSA bindings and a bounded worker queue;
browser playback uses an original AudioWorklet and reset-epoch ring. No external
Cargo packages or sound files were added. See [audio](AUDIO.md).

Local validation passed 373 workspace tests, strict native/Wasm Clippy, native
and Wasm release builds, all fifteen native gameplay smoke runs, docs/search and
browser-contract checks. A real-Wasm integration exercised each game's own
simulation/input path until all fifteen produced finite nonzero PCM. The core
playback/parameter/render regression allocates zero times after setup. An ALSA
null/file-device test checked actual system-output bytes and unavailable-device
handling. Live browser output showed nonzero PCM with zero reported underruns or
overflows during the observed Emberline run; mute/settings suspension and resume
were verified. This does not certify physical speakers, all devices or long-term
playback. No Windows/Apple device checks or iOS SDK checks were available.

The built-in bank uses **616,896 decoded sample bytes**, generated once at runtime.
It is absent from Wasm module/data download bytes. The runtime uses 64 voices;
queued device buffers, input/global state, renderer state and clips are separate
memory costs. Custom WAV sounds do not require generating the built-in bank.
The open audio output adds mixing to the normal update budget; fixed native
smoke/performance runs mix offline in simulation time instead of using hardware.

`target/perf/iteration-11-final/report.json` compares against iteration 10 using
unchanged release/default-quality workloads, two runs of 120 measured frames,
30 warmup frames, 20% timing allowance, 1,000 µs CPU/submission floor, 20 µs GPU
floor and 5% size allowance. Every timing and work-count gate passed. **54
artifact-size flags remain**: raw/gzip native files for the thirteen interactive
native games and raw/gzip Wasm for fourteen games. Courtyard Wasm remains within
the size allowance. These costs remain visible for the final size audit; neither
the baseline nor tolerances were increased to hide them.

| Game | Wasm bytes | Gzip-9 bytes | Raw increase vs 10 | Gzip increase vs 10 |
|---|---:|---:|---:|---:|
| owned-lab | 353,923 | 132,802 | +13,490 | +5,553 |
| owned-orbit | 246,854 | 93,840 | +22,176 | +9,112 |
| emberline | 256,004 | 98,301 | +23,845 | +9,873 |
| neon-swarm | 270,935 | 101,599 | +23,401 | +9,860 |
| pocket-rally | 243,273 | 92,462 | +23,677 | +9,564 |
| salvage-pinball | 365,888 | 139,292 | +22,546 | +8,972 |
| orchard-guard | 234,667 | 89,391 | +22,374 | +8,864 |
| marble-run | 387,985 | 148,794 | +22,894 | +9,051 |
| dune-skimmer | 311,163 | 120,088 | +22,879 | +8,986 |
| crystal-cavern | 338,658 | 130,117 | +23,210 | +9,560 |
| siege-foundry | 369,892 | 142,490 | +22,795 | +8,861 |
| sky-courier | 289,445 | 110,801 | +23,426 | +9,141 |
| strike | 350,670 | 134,760 | +24,006 | +9,465 |
| physics-2d | 360,864 | 137,907 | +22,891 | +9,143 |
| physics-3d | 413,058 | 157,972 | +23,197 | +9,534 |

An earlier measurement remains at `target/perf/iteration-11-before-suspend-fix`:
54 size flags plus a 2D lab GPU p99 flag (67.29 µs versus a 41.40 µs allowance).
The final source also clears transient voices when output is suspended and
primes loops to resume from silence; this fixes old one-shot tails after focus
loss. The GPU flag did not repeat in the final run. No rendering optimization or
GPU improvement is inferred from that difference. Native binary hashes in the
final report were checked against the current release files.

`target/iteration-11-emberline.wav` records six seconds / 288,000 stereo frames
of actual native gameplay mix. `audio_bank` generates an independent 6.42-second
preview. WAV captures, raw metrics and precompressed site copies stay outside
source/distribution size totals. Shared audio JavaScript is listed separately
by `scripts/report-size.py`.

## Iteration 12: animation playback and GPU skinning

Original animation now supplies skeletal/sprite players, shared clip banks,
paused/speed/seek clocks, moving/interrupted crossfades, masked/additive layers,
deterministic state graphs and bounded chronological events. Interrupted fades
snapshot the base blend before layers, preventing a reapplied aim/recoil layer
from being baked into the source twice. Preallocated skeleton evaluation and
validated position/normal palettes feed shared native GL/WebGL2 skinning, with
matching shadows and conservative animated bounds. Strike operators and weapons
and Emberline procedural sprites use the implementation. See [animation](ANIMATION.md).

Local validation passed **396 workspace tests**, strict native/Wasm Clippy,
native/Wasm release builds, strict Rustdoc, all fifteen native gameplay PNG smoke
runs, and docs/search/first-party audits. Seven real GLX tests include four-weight
skinning/nonuniform normals/shadows compared against an equivalent static model,
pose changes and GPU resource reuse/collection. Counting allocators report zero
allocations for core graph/layer/skeleton updates and warmed retained skin frames.
Real release Wasm recorded changing operators, accepted-shot recoil and a detached
reload magazine; all fifteen games also passed finite nonzero gameplay PCM checks
and shared input/storage contracts. Live browser Strike rendered skins and reload
without shader/console errors; Emberline retained its original artwork with timed
poses. Search found the new Animator API. No iOS or unavailable Windows/Apple
device checks ran.

`target/perf/iteration-12-final/report.json` records source commit
`a2b7a83628f3c44287457f7d6578dc2dc642a8d1` and compares against iteration 11 on
the same Linux/x86_64 machine, release/default quality, two runs of 120 measured
frames and 30 warmup frames. Tolerances remain 20% timing, a 1,000 µs
CPU/submission floor, 20 µs GPU floor and 5% size/work allowance. Current native
SHA-256 hashes and raw/deterministic-gzip Wasm lengths match the recorded artifacts.

**Twelve flags remain; the performance gate did not pass.** Four are Strike
artifact sizes, six are Strike draw/upload mean/p95/p99, and two are timing tails:
Courtyard software-render p99 **20,454.60 µs** versus **18,738.25 µs** allowed,
and Neon Swarm GPU p99 **83.84 µs** versus **44.20 µs** allowed. These remain
unresolved for the final audit. Neon Swarm draw/vertex/upload and allocation work
counts and Courtyard allocation counts/bytes match iteration 11; unchanged work
counts alone do not establish the cause of a timing difference.

Strike's independently animated poses usually require separate draws. Whole-game
mean draws rose **9 → 12.71**, p95/p99 **9 → 14**; mean uploads rose
**34,592 → 40,152.27 bytes**, p95/p99 **34,592 → 41,008 bytes**, including skin
palette uniforms in color/shadow programs. Mean CPU update cost was
**16.88 → 49.95 µs**; GPU mean **54.03 → 51.33 µs**. These are complete workload
observations with presentation changes, not isolated animation or GPU improvement
claims. Allocation call/byte distributions remained unchanged for Strike.

Native Strike measures **978,392 bytes / 473,912 gzip-9 bytes**, increases
of **+63,048 raw / +28,373 gzip** against iteration 11. No external
Cargo packages, images, skeleton files or animation downloads were added. Authored
tracks/geometry are generated by original game code; decoded/runtime pose storage
is separate from executable bytes.

| Game | Wasm bytes | Gzip-9 bytes | Raw change vs 11 | Gzip change vs 11 |
|---|---:|---:|---:|---:|
| owned-lab | 353,116 | 131,956 | -807 | -846 |
| owned-orbit | 246,135 | 93,009 | -719 | -831 |
| emberline | 261,667 | 100,082 | +5,663 | +1,781 |
| neon-swarm | 270,251 | 100,788 | -684 | -811 |
| pocket-rally | 242,589 | 91,623 | -684 | -839 |
| salvage-pinball | 365,204 | 138,530 | -684 | -762 |
| orchard-guard | 233,983 | 88,598 | -684 | -793 |
| marble-run | 388,430 | 148,694 | +445 | -100 |
| dune-skimmer | 311,608 | 119,915 | +445 | -173 |
| crystal-cavern | 339,101 | 129,986 | +443 | -131 |
| siege-foundry | 370,335 | 142,341 | +443 | -149 |
| sky-courier | 289,888 | 110,616 | +443 | -185 |
| strike | 406,548 | 154,611 | +55,878 | +19,851 |
| physics-2d | 360,180 | 137,160 | -684 | -747 |
| physics-3d | 413,501 | 157,815 | +443 | -157 |

Earlier evidence stays in `target/perf/iteration-12-before-buffer-fix` and
`target/perf/iteration-12-before-layer-fix`. The first contained Sky Courier
allocation-byte p99 **238 versus 190 baseline**, plus a 2D lab GPU p99 flag
(**72.54 µs versus 48.53 µs allowed**). Packing weights after positions in the
same immutable GPU buffer restored static mesh-cache entry size; subsequent
runs returned Sky Courier allocation p99 to **190**, matching its baseline.
The second report had ten Strike size/work flags and all timing gates passed.
The final run includes the additive-interruption fix and the two timing tails
above. The 2D lab final GPU p99 is **28.77 µs**, within its allowance; no GPU
optimization is inferred from the timing variation. Reports/thresholds were not
rebased or selectively overwritten to hide failed runs.


## Iteration 13: reusable effects

Shared simulation now covers seeded moving emitters, analytic drag, distance-sampled
trails, ribbons and bounded beams. Native GL/WebGL use instanced, texture-free
billboards (64 streamed bytes per particle instead of 192 for an expanded quad),
with sorted alpha, additive blending, conservative clip culling and explicit
ordering against opaque geometry and HUD draws. Triangle ribbons use the existing
32-byte vertex stream. Pools and simulation budgets are explicit; saturation is
counted. Simulation and warmed renderer recording each passed a 1,000-frame
zero-allocation test. This does not claim whole games or cold GPU setup allocate
nothing.

Validation: 413 regular workspace tests, nine actual GLX pixel/timer tests plus the
ALSA null-device check, strict native/Wasm Clippy, strict Rustdoc, all 15 native
release smoke checks and all 15 release Wasm builds. Real release Wasm exercised
effects in Strike, Neon Swarm, Dune Skimmer, Pocket Rally and Sky Courier, validating
finite instance data, ranges and flags; existing input/audio/animation contracts
also passed. Live browser checks rendered Dune dust, Sky exhaust and Rally trails
without console/shader errors. The dependency audit still finds 28 first-party
workspace packages and zero external Cargo packages. iOS and unavailable Apple/
Windows device checks were not run.

`target/perf/iteration-13-final/report.json` records source commit
`b5ddb5f7f6c588a59d784bc7b363ee7ce462880b` against iteration 12: release/default
quality, two runs of 120 measured frames after 30 warmup frames. Timing tolerance
remains 20%, CPU/submission floor 1,000 µs, GPU floor 20 µs, size/work allowance
5%. All native SHA-256 hashes and raw/deterministic-gzip Wasm lengths match the
recorded artifacts.

**Nineteen flags remain; the performance gate did not pass.** Twelve are artifact
sizes (ten Wasm/raw-gzip entries across the five adopting games, plus Sky native
raw/gzip); five are Neon work/allocation metrics; two are physics-lab GPU tails:
2D GPU p99 **70.93 µs > 48.77 µs** allowed, and 3D GPU p95
**102.72 µs > 58.35 µs** allowed. Prior iteration flags remain evidence for the
final audit; a later baseline does not resolve their costs.

Neon now splits arena, additive sparks and following HUD into up to three draws:
p95/p99 **1 → 3** (two allowed), mean **1 → 1.48**. Mean uploads were
**433,931.20 → 433,651.80 bytes**, with unchanged p95/p99 uploads/vertices.
Allocation p99 rose **15 → 21 calls**, mean bytes **227.30 → 259.04**, p99
bytes **860 → 1,112**. This short run includes the first particle draw and lazy
GPU/frame-buffer growth; isolated warmed recording is allocation-free, but the
whole-game flags remain. No causal allocation or timing improvement is inferred
from this comparison. Sky adds one exhaust draw and 448 instance bytes per frame
in this workload; its allocation distributions remain unchanged. Strike draw,
upload and allocation distributions match iteration 12.

The short catalog workload does not exercise all new effects: Dune remains in its
countdown, Rally has not yet drifted and Strike does not fire. A separate
`target/perf/iteration-13-active/report.json` records the same source with two
900-frame runs plus 30 warmup frames for Neon/Rally/Dune/Sky. It is supplemental,
not a replacement baseline or a passing comparison. Respective CPU p95 values
were **95.92 / 673.07 / 130.05 / 118.44 µs**, GPU p95
**22.24 / 98.99 / 58.71 / 33.46 µs**, and draw p95 **3 / 1 / 78 / 40**.
Strike firing and Sky boost trails are exercised by the real-Wasm effects test,
not claimed as covered by those native autoplay timing samples.

No textures or third-party runtime libraries were introduced. Download changes:

| Game | Wasm bytes | Gzip-9 bytes | Raw change vs 12 | Gzip change vs 12 |
|---|---:|---:|---:|---:|
| owned-lab | 358,292 | 134,054 | +5,176 | +2,098 |
| owned-orbit | 251,373 | 95,131 | +5,238 | +2,122 |
| emberline | 263,846 | 100,733 | +2,179 | +651 |
| neon-swarm | 297,600 | 110,911 | +27,349 | +10,123 |
| pocket-rally | 261,867 | 98,801 | +19,278 | +7,178 |
| salvage-pinball | 367,383 | 139,137 | +2,179 | +607 |
| orchard-guard | 236,162 | 89,264 | +2,179 | +666 |
| marble-run | 390,617 | 149,281 | +2,187 | +587 |
| dune-skimmer | 344,396 | 132,181 | +32,788 | +12,266 |
| crystal-cavern | 341,288 | 130,610 | +2,187 | +624 |
| siege-foundry | 372,519 | 142,920 | +2,184 | +579 |
| sky-courier | 340,636 | 129,346 | +50,748 | +18,730 |
| strike | 443,268 | 167,937 | +36,720 | +13,326 |
| physics-2d | 362,359 | 137,806 | +2,179 | +646 |
| physics-3d | 415,685 | 158,485 | +2,184 | +670 |

Sky native measures **835,112 bytes / 404,824 gzip-9 bytes**. Strike native
measures **1,010,344 / 491,272 bytes**. All raw reports remain in ignored target
storage. Thresholds and earlier reports were not overwritten to hide flags.


## Iteration 14: shared AI and reusable navigation

Sight cones, occluded hearing, expiring identity memory, bounded priority selection,
ordered decision graphs and round-robin sensing now live in ferravex-play. An
indexed-heap incremental A* implementation reuses its per-cell/heap/path storage;
a FIFO planner shares scratch across agents. Search expansion and parent tracing
both consume a per-tick work allowance. Polyline distance cursors and collision-
driven waypoint followers share reusable routes. Grid identity/revision checks
reject changed search inputs. The engine adds a current-pose rigid visibility
adapter. Strike, Neon Swarm and Orchard Guard use the shared systems.

Local validation passed **429 workspace tests** (10 device/GPU tests remain ignored
in the regular suite, as before), strict native and Wasm Clippy, strict Rustdoc,
all 15 native release smoke checks at 360 frames and all 15 release Wasm builds.
Existing real-Wasm input/audio/animation/effects checks also passed, including
changing Strike bot poses and accepted-shot effects. Core tests compare incremental
paths with reference A* on 48 weighted/disconnected maps in both connectivity
modes. A counting allocator recorded **zero allocations across 1000 constructed
AI/search/follower updates**. Game tests route a pursuer around cover, prevent
hidden silent target knowledge/fire, accept bot shots through Match, and win both
complete Neon/Orchard autoplay scenarios. This does not claim whole-game allocation
freedom. Static site/search checks and the 28-package, zero-external-Cargo-package
audit passed. Live browser gameplay in all three adopting games rendered without
console/shader errors; the local host served game/Wasm assets with HTTP 200.
No iOS or unavailable Windows/Apple device checks were run.

`target/perf/iteration-14-final/report.json` records source
`a45b97a0d016e4f1f8e435f7c02d48fa641261ce`, compared against iteration 13 with
unchanged release/default-quality configuration: two 120-frame samples after
30 warmup frames, 20% timing tolerance, 1000 µs CPU/submission floor, 20 µs GPU
floor and 5% size/work tolerance. All native SHA-256 hashes and raw/deterministic-
gzip Wasm lengths match the recorded artifacts.

**Four size flags remain; the overall performance gate did not pass.** They are
raw/gzip Wasm for Neon and Strike. All timing/work gates passed with those existing
thresholds; this does not mean all CPU costs stayed flat. Neon whole-game CPU mean
rose **59.28 → 81.37 µs**, p95 **68.57 → 99.33 µs**; Orchard mean
**239.34 → 299.39 µs**, p95 **299.00 → 554.00 µs**. These changed gameplay
workloads include sensing/navigation/targeting and are not isolated AI benchmarks.
Neon draw counts remain p95/p99 **3** and whole-game allocation p99 remains
**21 calls**; byte p99 **1112 → 1144**. Orchard draw/upload/allocation distributions
remain unchanged.

Strike's command vector is reused rather than allocated each fixed step. Its
measured whole-game allocation mean changes **3.075 → 2.008 calls / 192.73 →
13.53 bytes**, p95/p99 **4 → 2 calls / 341 → 5 bytes**. CPU mean was
**45.84 → 40.91 µs**, p95 **61.79 → 56.19 µs**; GPU mean
**51.11 → 51.67 µs**. Bots now follow routes and visibility, so fewer visible
actors/draws in this sample are a workload difference, not proof of faster rendering.
Whole-game p95 draws **14 → 13**, uploads **41008 → 40208 bytes**.

A separate longer `target/perf/iteration-14-active/report.json` samples the three
adopting games for two 900-frame runs plus 30 warmup frames. It supplements the
short catalog comparison, does not replace it and is not a baseline pass:

| Game | CPU p95 µs | GPU p95 µs | Draw p95 | Allocation calls p95 |
|---|---:|---:|---:|---:|
| neon-swarm | 107.47 | 22.16 | 3 | 13 |
| orchard-guard | 308.25 | 29.02 | 1 | 6 |
| strike | 73.30 | 55.74 | 16 | 3 |

Native Neon measures **840696 / 403333 gzip-9 bytes**; Orchard **754928 /
363733**; Strike **1032792 / 502492**. No runtime asset downloads were added.

| Game | Wasm bytes | Gzip-9 bytes | Raw change vs 13 | Gzip change vs 13 |
|---|---:|---:|---:|---:|
| owned-lab | 358,292 | 134,127 | +0 | +73 |
| owned-orbit | 251,373 | 95,129 | +0 | -2 |
| emberline | 263,846 | 100,739 | +0 | +6 |
| neon-swarm | 323,855 | 120,781 | +26,255 | +9,870 |
| pocket-rally | 261,867 | 98,802 | +0 | +1 |
| salvage-pinball | 367,383 | 139,133 | +0 | -4 |
| orchard-guard | 241,031 | 91,119 | +4,869 | +1,855 |
| marble-run | 390,617 | 149,286 | +0 | +5 |
| dune-skimmer | 344,396 | 132,184 | +0 | +3 |
| crystal-cavern | 341,288 | 130,612 | +0 | +2 |
| siege-foundry | 372,519 | 142,924 | +0 | +4 |
| sky-courier | 340,636 | 129,338 | +0 | -8 |
| strike | 467,231 | 177,764 | +23,963 | +9,827 |
| physics-2d | 362,359 | 137,816 | +0 | +10 |
| physics-3d | 415,685 | 158,493 | +0 | +8 |

Earlier size/work/timing flags, including iteration 13's Neon draw/cold-allocation
costs and physics-lab GPU tails, remain evidence for the final audit. A later
baseline or an unflagged sample does not resolve them. Reports and thresholds
were not overwritten to hide failed comparisons.


## Iteration 15: typed scenes, prefabs and content reload

The new first-party authoring system validates typed components against explicit
schemas, expands hierarchical prefabs with child overrides/scoped references,
builds exact world matrices and supports checksummed cooked data. Native explicit
file watching and a browser/Wasm editor stage complete replacements before adoption.
Invalid edits preserve the current game and generation. Orchard Guard now consumes
an authored route/plots/campaign/economy; Marble Run consumes authored course shapes,
checkpoints, route references, pickups and physics tuning. Identical prop/obstacle
geometry is shared; rendered platform orientation/dimensions match its collider.

Validation passed 444 regular workspace tests, including a fresh full rerun after
the HUD cache fix below; strict native/Wasm Clippy and Rustdoc; all
15 native release smoke checks; all 15 Wasm builds; existing real-Wasm input/audio/
animation/effects contracts; site/search checks; and the 28-package zero-external-
Cargo-package audit. Nine GLX pixel/timer checks and the ALSA null-device check also
ran successfully on this Linux host. Authoring tests cover schema/type/range errors,
integer precision, namespace isolation, references, unused/nested prefab cycles,
world overflow, expansion/depth budgets, deterministic cooking and corrupt binary
semantics after a valid checksum. Native tests cover same-length file edits,
transactional rejection and shared mesh/collider transforms.

Real release Wasm for both editable games rejected invalid edits, adopted twelve
valid generations, kept finite frame data and released retired mesh ownership.
The supplied web builder separates editing-enabled Cargo feature cohorts; the same
test confirms the editing ABI is absent from the other thirteen Wasm games. Live
browser checks showed rejected edits preserving Marble's old scene, then a wider
platform/larger marble at generation 1; Orchard's accepted data changed its visible
lives/currency. No console/shader errors appeared. No iOS or unavailable Apple/
Windows device checks were run.

`target/perf/iteration-15-final/report.json` records source commit
`763b0b6814bf2d69922c77452dc5cb7dfd04ca7c` against iteration 14. Configuration remains
release/default quality, two 120-frame runs after 30 warmup frames, 20% timing
allowance, 1000 µs CPU/submission floor, 20 µs GPU floor and 5% size/work tolerance.
All native SHA-256 hashes and raw/deterministic-gzip Wasm lengths match current
artifacts. These are warmed gameplay samples: initial parsing/construction and
interactive reload latency are not timed by this comparison. Native file watching
is disabled unless explicitly requested; browser editor JS loads only with
`?content-tools`.

**Eight artifact-size flags remain; the overall performance gate did not pass.**
They are raw/gzip native and Wasm for Orchard and Marble. All timing and work gates
passed with unchanged thresholds. Those examples deliberately retain their text
parser, schemas, embedded authoring text, diagnostics and reload support. A shipping
host can load only cooked data and omit the editing feature; that smaller variant
is an available API choice, not claimed as the artifact measured here.

Orchard's whole-game allocation distributions match iteration 14: **6 calls and
92 bytes per frame** for mean/p95/p99. Draws remain **1**, with identical vertex/
upload distributions. CPU mean was **299.39 → 203.40 µs**, p95 **554.00 →
222.02 µs**; GPU mean **25.77 → 25.45 µs**. These are complete workload observations,
not proof that parsing or scene traversal became faster.

Marble allocation mean/p95/p99 remains **3 calls / 56 bytes**, with unchanged
vertex/upload distributions. Sharing the checkpoint pole/flag meshes reduces
whole-game mean draws **116.53 → 114.53**, p95/p99 **117 → 115**. CPU mean was
**123.24 → 119.86 µs**, p95 **139.79 → 129.00 µs**; GPU mean
**27.22 → 26.02 µs**. No isolated GPU timing improvement is inferred from these
small differences.

A supplemental longer two-game workload in
`target/perf/iteration-15-active/report.json` records two 900-frame runs plus
30 warmup frames. It supplements rather than replaces the failed comparison:

| Game | CPU p95 µs | GPU p95 µs | Draw p95 | Allocation calls/bytes p95 |
|---|---:|---:|---:|---:|
| orchard-guard | 313.79 | 27.25 | 1 | 6 / 92 |
| marble-run | 130.70 | 32.50 | 115 | 3 / 56 |

Native Orchard measures **950200 / 462806 gzip-9 bytes**, growth of
**195272 / 99073** versus 14. Native Marble measures **1148416 / 558670 bytes**.
The embedded source files are **3126 bytes** (Orchard) and **5256 bytes** (Marble).
No new normal-gameplay asset downloads were introduced. The optional scene-editor
JS is **4211 raw / 1758 deterministic-gzip bytes**, fetched only when requested.

| Game | Wasm bytes | Gzip-9 bytes | Raw change vs 14 | Gzip change vs 14 |
|---|---:|---:|---:|---:|
| owned-lab | 358,292 | 134,131 | +0 | +4 |
| owned-orbit | 251,373 | 95,062 | +0 | -67 |
| emberline | 263,923 | 100,704 | +77 | -35 |
| neon-swarm | 323,917 | 120,735 | +62 | -46 |
| pocket-rally | 261,950 | 98,748 | +83 | -54 |
| salvage-pinball | 367,465 | 139,089 | +82 | -44 |
| orchard-guard | 422,156 | 167,827 | +181,125 | +76,708 |
| marble-run | 603,552 | 232,867 | +212,935 | +83,581 |
| dune-skimmer | 344,479 | 132,133 | +83 | -51 |
| crystal-cavern | 341,381 | 130,650 | +93 | +38 |
| siege-foundry | 372,604 | 142,891 | +85 | -33 |
| sky-courier | 340,714 | 129,285 | +78 | -53 |
| strike | 467,231 | 177,768 | +0 | +4 |
| physics-2d | 362,440 | 137,789 | +81 | -27 |
| physics-3d | 415,757 | 158,434 | +72 | -59 |

The earlier `target/perf/iteration-15-before-hud-cache/report.json`, source
`bc4584d`, retained twelve flags: eight sizes, three Orchard allocation-byte
metrics (**144 versus 92 baseline**), and a 2D lab GPU p99 tail (**72.78 µs >
49.30 µs allowed**). A dynamically formatted wave hint caused the reproducible
52-byte frame increase; caching it in the accepted level restored the baseline
allocation distributions. The final physics-lab sample is within its allowance,
but its GPU code was not optimized and the earlier tail remains evidence. Earlier
iteration size/work/timing costs also remain for the final audit. Neither reports
nor thresholds were overwritten to hide failed comparisons.

## Iteration 16: bounded shared HUD and menus

The renderer now shares clipped panels/gradients, aligned fitted/wrapped bitmap
text, buttons, modal interaction scopes and a persistent formatting arena. It
uses the existing screen-space immediate stream on native GL and WebGL2 without
changing world camera/material/depth state. Horizontal bitmap runs replace
individual lit-pixel rectangles. Strike's HUD/menu/roster, Neon's HUD/menus and
Orchard's build/upgrade/wave/restart controls adopt it. The navigation adapter
honors remapped player confirmation as well as fixed host navigation; ordered
mouse positions and touch IDs preserve capture and release semantics. See [UI](UI.md).

`target/perf/iteration-16-final/report.json` records production source
`82dd3d056a57cb28dbd507986bc35b4df30fba0c`, compared with the unchanged
iteration-15-final baseline. All fifteen games ran twice with 30 warmup and 120
measured frames each (240 samples/game). The existing allowances remain 20%, a
1000 µs CPU/submit/software floor, 20 µs GPU floor and 5% artifact growth. Native
hashes and raw/deterministic gzip-9 sizes for all native/Wasm artifacts match the
report; staged browser Wasm matches those measured artifacts. No builds, tests or
agent-created game tabs ran alongside this measurement.

**The comparison gate failed with four flags**, retained in the report:

- Neon Wasm grew from 323917 to 352752 bytes; gzip from 120735 to 132646.
- Orchard Wasm grew from 422156 to 444533 bytes. Its gzip grew from 167827 to
  175802, within the existing 5% allowance.
- The unchanged 3D physics lab workload had GPU p95 103.837 µs versus 31.259 µs,
  above its 51.259 µs allowance. Its geometry, upload, allocation and simulation
  work distributions are identical. This tail was not isolated or fixed here.

Artifact costs for the three adopting games (raw / gzip-9 bytes):

| Game | Native before → after | Wasm before → after |
|---|---|---|
| Strike | 1032824 / 502568 → 1056520 / 514285 | 467231 / 177768 → 486967 / 184843 |
| Neon Swarm | 841584 / 403752 → 864768 / 415455 | 323917 / 120735 → 352752 / 132646 |
| Orchard Guard | 950200 / 462806 → 976992 / 476293 | 422156 / 167827 → 444533 / 175802 |

Measured whole-game changes, not isolated UI microbenchmarks:

| Game | CPU mean / p95 µs before → after | Mean vertices before → after | Mean upload bytes before → after |
|---|---|---|---|
| Strike | 45.152 / 61.468 → 41.659 / 57.809 | 24228.6 → 23880.6 | 39998.93 → 28862.93 |
| Neon Swarm | 63.615 / 79.817 → 43.831 / 55.278 | 13536.9 → 10175.9 | 432944.6 → 325392.6 |
| Orchard Guard | 203.404 / 222.023 → 182.142 / 203.584 | 41890.9 → 38470.9 | 1340508.8 → 1231068.8 |

Draw-call distributions are unchanged for these games. Strike removed two HUD
formatting allocations and 5 bytes per frame: mean calls 2.0083 → 0.0083, p95/p99
2 → 0; mean bytes 13.533 → 8.533, p95/p99 5 → 0. Two sampled frames still each
allocate 1024 bytes elsewhere in the game. Neon removed four HUD allocations and
40 bytes/frame: means 11.2333 → 7.2333 calls and 259.575 → 219.575 bytes; it still
allocates elsewhere. Orchard remains 6 calls / 92 bytes per frame because its
legacy HUD formatting remains. A separate allocator test records **zero calls**
across 1000 warmed shared-UI layout/format/clip/paint frames. Each Ui reserves
213504 CPU bytes on this 64-bit host, plus the renderer's shared geometry stream;
startup/worst-case stream growth and UI construction are outside the warmed test.

No claim of flat overall timing follows from passing the generous timing floors.
For example, physics-3d CPU p95 increased 1354.279 → 1848.327 µs (within its 1000 µs
floor); Orchard submit p99 increased 117.706 → 707.230 µs. All earlier iteration
size/work/timing costs remain for the final audit. This baseline does not resolve
or erase them, and the GPU flag above remains open.

Local validation: 454 regular workspace tests; 11 separately enabled X11/GLX/ALSA
checks including actual UI clip/paint-order pixels; strict native and Wasm library
Clippy and Rustdoc; all fifteen native release bins and Wasm games; all fifteen
360-frame native gameplay/PNG checks; existing real-Wasm input/audio/animation/
effects/content contracts; new real-Wasm UI press/release, same-frame drag-out,
multi-touch, focus, settings blocking and actual Orchard build/wave integration;
and 21 site/search/audit/performance Python tests. Browser visual checks confirmed
Neon keyboard start/focus, Orchard building/help spacing and Strike's smooth fade,
with no captured warnings/errors. No iOS or unavailable native device tests ran.

## Iteration 17: typed saves, persistent preferences and records

The first-party save container validates versions, schema, bounded fields and
checksums before accepting values. Native storage uses synced same-directory
replacement; browser storage uses a bounded game-scoped localStorage bridge.
All fifteen examples persist audio/graphics preferences through the existing F2
Devices panel. Existing FXIN control profiles retain their format and paths,
with native writes sharing the new atomic-file code. Rally persists best laps
and ghosts; Marble and Neon persist records; Orchard validates and restores safe
between-wave progress. Details and storage limitations: [saves](SAVES.md).

`target/perf/iteration-17-final/report.json` records source
`c3eb5897dbc8cf914beccf98dc93cf5b2df047cc` against the unchanged iteration-16-final
baseline. The same fifteen workloads ran twice, with 30 warmup and 120 measured
frames/run. Existing 20% / 1000 µs CPU/submit/software floors, 20 µs GPU floor and
5% size allowance are unchanged. All native hashes and native/Wasm raw and
deterministic gzip-9 sizes match the report; staged Wasm matches. No builds,
tests or agent-created game tabs ran alongside the measurements.

**The comparison gate failed with 20 flags: 19 Wasm-size flags and one GPU tail.**
Raw Wasm exceeds the allowance for Courtyard, Orbit, Emberline, Neon, Rally,
Pinball, Orchard, Dune, Cavern, Sky and the 2D physics lab. Gzip exceeds it for
Orbit, Emberline, Neon, Rally, Orchard, Cavern, Sky and the 2D physics lab. No
native-size flag remains, but native artifacts also grew. The 2D physics lab GPU
p99 is 72.698 µs versus 27.372 µs (47.372 µs allowance); its draw/upload/geometry,
allocation and simulation-work distributions are identical. That tail was not
isolated or fixed in this iteration.

Artifact costs (raw / gzip-9 bytes):

| Game | Native before → after | Wasm before → after |
|---|---|---|
| Strike | 1056520 / 514285 → 1077688 / 524274 | 486967 / 184843 → 504414 / 191265 |
| Neon Swarm | 864768 / 415455 → 895144 / 429727 | 352752 / 132646 → 377020 / 141314 |
| Pocket Rally | 775144 / 373168 → 804408 / 387562 | 262507 / 98878 → 286906 / 108283 |
| Orchard Guard | 976992 / 476293 → 1018336 / 496506 | 444533 / 175802 → 478035 / 188607 |
| Marble Run | 1148704 / 558911 → 1181360 / 574353 | 604109 / 232975 → 629960 / 242583 |

In ordinary sampled gameplay, **available allocation, draw, vertex, upload and
simulation-work distributions match the baseline across all fifteen games**. Real-Wasm transport
counters separately confirm that 120 unchanged frames per game perform no
storage reads/writes. These facts do not make saving free: load/encode/commit
allocate and synchronous native sync/localStorage calls can stall a save frame.
The short warmed workload and transient autoplay mode do not measure storage
latency, full-recording memory, startup loading or a durable end-of-run save.

Observed whole-game CPU mean / p95 / p99 µs, before → after:

| Game | Before | After |
|---|---|---|
| Strike | 41.659 / 57.809 / 75.548 | 41.654 / 56.548 / 101.587 |
| Neon Swarm | 43.831 / 55.278 / 74.068 | 47.105 / 64.958 / 100.387 |
| Pocket Rally | 482.742 / 503.386 / 537.505 | 513.440 / 533.413 / 548.542 |
| Orchard Guard | 182.142 / 203.584 / 242.583 | 204.606 / 225.833 / 629.719 |
| Marble Run | 126.580 / 142.176 / 272.182 | 98.542 / 110.536 / 289.350 |

Passing the generous timing floors is not evidence that CPU timings stayed flat;
several means and tails increased. These measurements do not isolate the cost of
preference sampling from compiler layout or machine variation. Earlier iteration
costs remain open. In particular, physics-3d GPU p95 is now 32.141 µs, but its
iteration-16 103.837 µs flag was not fixed by this storage change and is retained.

The first build/report is preserved at
`target/perf/iteration-17-before-field-vector/report.json`, source
`08a308de5ee5b2ded2ed878a3d97395a89334aaf`. It had 29 flags: 28 artifact-size
flags and Courtyard software-render p99 21714.778 µs versus its 19128.581 µs
allowance. Replacing the sorted save-field tree with a bounded sorted vector
reduced raw Wasm by **77410 bytes across the fifteen artifacts** (roughly 5–6 KiB
per game) without changing wire bytes or limits. The original timing tail remains
evidence: the software renderer was not optimized; final p99 16274.859 µs is
within the allowance but does not establish a causal rendering improvement.
Neither report nor thresholds were overwritten to erase a failed comparison.

Local validation: 469 regular workspace tests; 11 separately enabled GLX/ALSA
checks; strict native and Wasm library Clippy and Rustdoc; all fifteen native/Wasm
release builds and all fifteen 360-frame native gameplay/PNG checks; existing
real-Wasm input/audio/animation/effects/content/UI contracts; new browser storage
bounds/quota/scope/memory-growth contracts; all fifteen real-Wasm preference
editing/reload/mute/quality and idle-I/O checks; actual Orchard checkpoint
reload/restart; actual Neon completed-attempt counts across fresh Wasm instances;
Rally's real simulated lap ghost roundtrip and Orchard's resumed campaign test;
and 21 site/search/audit/performance Python tests. Browser checks confirmed
Orchard progress and volume/mute/quality reload, plus compatibility with a save
written before the field-vector change, with no captured warnings/errors. Apple
and Windows runtime/device checks were unavailable; iOS SDK/device checks remain
excluded. No external Cargo packages were added.

## Iteration 18: replay timelines, ghosts and deterministic shooter aim

Practice recording now stores full authority checkpoints, all player commands,
accepted events and per-tick hashes. Playback uses a separate match, bounded
checkpoint seeking and visible desync errors. Rally and Strike share sparse
2D/3D ghost interpolation. The optional browser replay panel supports import and
export; native bounded runs can export `.fxrt`. See [replays](REPLAYS.md).

`target/perf/iteration-18-final/report.json` measures source
`01f62a0d3ce524a03bb76de411a0ab3fd3e0927f` against iteration-17-final: fifteen
workloads, two runs, 30 warmup and 120 measured frames/run. The existing 20% /
1000 µs CPU/submit/software floors, 20 µs GPU floor and 5% size allowance are
unchanged. Native binary hashes, native/Wasm raw and deterministic gzip-9 sizes,
and staged Wasm were checked against the report. No builds, tests or
agent-created game tabs ran alongside the measurements.

**The gate failed with four Strike artifact-size flags.** No timing or work
metric triggered a new flag. Other games stayed inside their comparison budgets;
several artifacts still grew. Artifact costs (raw / gzip-9 bytes):

| Game | Native before → after | Wasm before → after |
|---|---|---|
| Strike | 1077688 / 524274 → 1151512 / 559679 | 504414 / 191265 → 567722 / 214999 |
| Pocket Rally | 804408 / 387562 → 808784 / 389690 | 286906 / 108283 → 291387 / 109881 |
| Courtyard | 571048 / 281131 → 571368 / 281399 | 377580 / 140958 → 378218 / 141318 |

All available allocation, draw, vertex, upload and simulation-work distributions
match the baseline across all fifteen games. This does not establish unchanged
CPU cost. Strike CPU mean / p95 / p99 changed from 41.654 / 56.548 / 101.587 to
46.601 / 65.728 / 100.767 µs; Rally changed from 513.440 / 533.413 / 548.542 to
545.197 / 697.081 / 761.519 µs. The timing floors remain permissive. Earlier
artifact/work/timing regressions, including the iteration-17 2D physics GPU p99
flag and iteration-16 3D physics GPU p95 flag, remain open. This new baseline does
not fix or erase them.

Recorder pushes and shared ghost sampling allocate nothing after explicit
reservation (1,000-tick allocator test). Recording reserves several MiB up front;
checkpoint restoration, encoding, import/export and synchronous saves allocate
and can stall. The normal warmed workload has recording disabled, no complete
best lap, and transient saves. It does not measure recording CPU cost, full-track
memory, startup restoration, durable I/O latency or replay-seek spikes.

An extra cross-runtime experiment exposed a native → Wasm desync at tick 19.
Replacing shooter movement/aim libm trig with shared fixed-polynomial arithmetic
fixed that experiment: a new 640-tick native clip completes actual Wasm playback
with every hash verified. The authority context identifies the changed rules.
The failed experiment is retained here; arbitrary CPU/compiler/rules equivalence
is not claimed. Future incompatibility still halts playback visibly.

Local validation: 479 regular workspace tests; 11 available GLX/ALSA checks;
strict native and Wasm library Clippy and Rustdoc; all fifteen release native/Wasm
builds and 360-frame native gameplay/PNG checks; all existing Node input, audio,
animation, effects, content, UI and persistence contracts; actual-Wasm recording,
durable reload, seeking, pause, bounded import/export, and bad-import preservation;
640-tick native export → Wasm playback; native CLI record and full playback;
real combat checkpoint/rewind/seek continuation including tick wrap; 1,000-tick
allocation checks; and 21 Python site/search/audit/performance tests. Browser
interaction confirmed record/save/replay/pause/one-second seek and panel layout.
No external Cargo packages were added. Windows/Apple runtime/device checks were
unavailable; iOS SDK/device checks remain excluded.


## Iteration 19 — multiplayer integration

Measured source `0bdd2afe2da96cd02a7ede903fb00a5620b3d412`, compared with
iteration 18's final report. Raw evidence: ignored
`target/perf/iteration-19-final/report.json` and per-game frame records.
All fifteen games, two runs of 120 measured frames after 30 warmup frames,
release profile, default quality. Existing budgets stayed at 20% with a 1,000 µs
CPU floor, 20 µs GPU floor, and 5% artifact growth. No test/build/game-tab activity
ran alongside the benchmark.

**Comparison failed with three flags.** Strike's Wasm increased from 567,722 to
704,609 raw bytes, and from 214,999 to 269,994 gzip-9 bytes. It now includes the
same client/authority session, clock/delta/reliable protocols, tactical rules,
byte-link exports and objective presentation. Native Strike increased from
1,151,512 / 559,679 to 1,198,056 / 583,364 raw/gzip bytes (within this step's 5%
budget). The optional browser tools are separate host files. Feature availability
has a distribution cost; it is not a size improvement.

Physics-3D GPU p95 rose from 37.63 to 116.381 µs against a 57.63 µs allowed
value. GPU coverage was 100% (240 samples); p99 was 117.342 µs. The cause has not
been isolated. This remains open alongside the earlier physics GPU-tail flags;
no baseline or threshold was changed to hide it.

Strike CPU mean/p95/p99 changed from 46.601 / 65.728 / 100.767 to
53.442 / 71.227 / 98.387 µs. Physics-2D changed from 622.985 / 1162.848 /
1723.123 to 728.704 / 1916.486 / 1937.495 µs, and Physics-3D from 649.656 /
1848.959 / 1889.638 to 733.893 / 1885.216 / 2391.392 µs; these stayed within the
unchanged time budgets. All available allocation, draw, vertex, upload and
simulation-work distributions were identical to iteration 18 for every game.
Unavailable metrics remain unavailable. Native hashes/raw/gzip sizes and staged
Wasm bytes were checked against the report.

The catalog benchmark uses offline practice. It does **not** measure active
multiplayer bandwidth, authority cost at sixteen players, adverse Internet
conditions, gateway load or lower-end hardware frame rates. Separate functional
checks ran two actual Wasm endpoints with loss/duplication/reordering, a one-Hz
background client, two Wasm clients through the native WebSocket/UDP authority,
and native UDP timed shots/clock/delta recovery, reliable commands and complete
plant/defuse/round-reset/forfeit transitions. Browser interaction verified two
identities, late-purchase rejection and accepted bomb drop. Background browser
throttling still caused roughly 1–2 second RTT in this UI environment; the
compacted queues recovered without the previous retry backlog.

Local validation: 494 regular workspace tests, 11 available GLX/ALSA checks,
strict native/Wasm Clippy and Rustdoc, all fifteen release native/Wasm builds and
360-frame native gameplay/PNG checks, all Node game contracts, a 640-tick native
practice replay verified in Wasm, and 21 Python checks. Fixed-step native smoke
runs now disable vsync so occluded compositor windows cannot stall automation.
The entire workspace still has 28 first-party Cargo packages and zero external
Cargo packages. Windows/macOS runtime checks were unavailable; iOS checks remain
excluded. No deployment command was issued.

## Iteration 20 — native platform hosts

Measured source `b8fd9af868dbb95f34c28deeb61ae24054bbbae6`, compared with
iteration 19. Evidence: ignored `target/perf/iteration-20-final/report.json`.
All fifteen games, two runs of 120 measured frames after 30 warmup frames,
release profile. Budgets remain 20%, 1,000 µs CPU floor, 20 µs GPU floor and
5% artifact growth. The harness now explicitly sets `FERRAVEX_DPI_SCALE=1`
to preserve the historical framebuffer workload; production still honors the
system DPI. No builders, tests or game tabs ran alongside the measurements.

**The final comparison passed with no flags.** An initial comparison at source
`76bbf6fc2c0fdb1d2cdc2151e58db067365873cf` failed ten native gzip-size budgets.
That report is preserved in `target/perf/iteration-20-before-dpi-parser`.
Replacing general float parsing for Linux DPI configuration with a bounded
first-party decimal reader removed about 17,752 raw / 14,800–15,000 compressed
bytes from each windowed example. For example, Emberline decreased from
825,224 / 406,469 to 807,472 / 391,668 raw/gzip bytes; Strike decreased from
1,231,480 / 606,721 to 1,213,760 / 591,724. Required decimal, fractional,
exponent, malformed and overflow inputs are covered. This is a real code
reduction, not a changed size threshold or baseline.

Strike CPU mean/p95/p99 is 53.216 / 71.817 / 102.557 µs. Physics-2D is
656.428 / 1174.145 / 1914.374 µs, Physics-3D 718.552 / 1859.065 / 1894.424 µs,
and Rally 691.400 / 894.833 / 1210.485 µs. Physics-3D GPU p95/p99 is
29.976 / 116.661 µs with 100% coverage. A lower p95 in this run does not
isolate or resolve the previously recorded GPU-tail issues. Earlier cumulative
artifact and timing costs remain open.

All available work distributions match iteration 19 except Neon Swarm allocation
bytes: mean 219.575 → 218.817, maximum 4149 → 4058, with unchanged median,
p95, p99 and allocation-call distribution. The small difference has not been
isolated. Native hashes and raw/gzip sizes, Wasm sizes, and staged Wasm bytes
were verified against the report. This is the same available Linux machine,
not evidence of frame rates on low-end hardware or Apple/Windows devices.

Local validation: 495 regular workspace tests; 13 available Linux GLX/ALSA
checks, including repeated graphics ownership and audio suspension; strict
native, Windows GNU, macOS ARM64 and Wasm Clippy; strict Rustdoc/site build;
all fifteen release native/Wasm builds and 360-frame gameplay/PNG checks;
Node contracts, actual-Wasm material fallback and multiplayer/replay checks;
and 21 Python checks. Browser interaction exercised actual WebGL context loss
and restoration, preserved game progress and failed-edit status. A native GPU
pixel test proves the last valid custom shader survives recreation. Cross-checks
for Windows/macOS do not prove linking or runtime behavior. iOS source and
bundle manifests are included provisionally; iOS SDK/device testing is excluded.
All 28 Cargo packages remain first-party. No deployment command was issued.

## Post-20 — demo polish and cheaper presentation

Measured source `865c3ee7c47a7b5782829b065d496dbb19d1f195`, compared with the
iteration-20 final report. Evidence: ignored `target/perf/polish-final/report.json`
and `target/perf/polish-low/report.json`. Each covers all fifteen games, two runs
of 120 measured frames after 30 warmup frames. The default comparison retains
the same release profile, framebuffer sizes and budgets: 20%, 1,000 µs CPU floor,
20 µs GPU floor, 5% artifact/work growth. No builds, tests or agent game tabs ran
alongside either benchmark. The available Linux GPU is an AMD Radeon 8060S using
Mesa; it is not a substitute for testing lower-end physical devices.

**The final default comparison passed with no flags.** The first polish report,
source `c4f051f3ca31633dda01c2a047b96a50d671106f`, remains in
`target/perf/polish-default`. It failed three cave-vertex budgets after spatial
batching, plus Neon Swarm GPU p99 (83.238 µs versus 21.480 µs previously).
Removing cave faces fully hidden by neighboring solid cells, after the original
AO bake, eliminated the extra vertex work. Neon's later p99 was 21.400 µs;
no Neon GPU code changed between those reports, so the earlier intermittent
spike remains unexplained. Previously recorded physics GPU tails, cumulative
artifact growth and other open regressions have not been erased or declared fixed.

| Example / metric | Iteration 20 | Polished default |
|---|---:|---:|
| Courtyard allocation bytes/frame, mean | 4,157,068.533 | 364.533 |
| Courtyard software raster time, mean | 14,742.387 µs | 13,988.427 µs |
| Orbit Breaker software raster time, mean | 629.309 µs | 527.894 µs |
| Crystal Cavern draw calls/frame, mean | 118.158 | 16.142 |
| Crystal Cavern submitted vertices/frame, mean | 26,025.350 | 25,123.600 |
| Crystal Cavern CPU / submission, mean | 126.098 / 195.626 µs | 105.491 / 72.594 µs |
| Crystal Cavern GPU, mean | 63.248 µs | 51.148 µs |
| Strike draw calls/frame, mean | 12.592 | 11.592 |
| Strike submitted vertices/frame, mean | 23,880.600 | 23,316.600 |

Courtyard now reuses framebuffer/depth/scratch storage; its reference rasterizer
still costs about 14 ms at 960×540 on this machine. Orbit uses opaque row fills.
Cave chunks trade individual-piece culling for fewer submissions, with hidden
faces removed before upload. Strike removes duplicate baked shadow quads and
excludes its foreground weapon from world shadow/depth work. Its default GPU
mean increased 48.712 → 53.845 µs while CPU/submission means are 53.405 / 61.984 µs.
Extra presets and original weapon silhouettes are features with costs, not a
claim that every metric improved. Strike ships 1,219,576 / 593,881 native
raw/gzip bytes and 710,084 / 271,908 Wasm raw/gzip bytes; all fifteen games stayed
within this step's existing artifact budgets.

The separate Low-quality experiment uses the same binaries and workloads but a
**different quality configuration**, so it is not gated against default quality.
Strike default → Low mean draws are 11.592 → 6, vertices 23,316.6 → 11,865,
submission 61.984 → 46.341 µs and GPU 53.845 → 38.579 µs. Dune mean draws are
60 → 23, vertices 38,366.5 → 29,036.5, submission 122.760 → 85.369 µs and GPU
54.482 → 43.509 µs. Crystal keeps the same geometry and has GPU mean
51.148 → 44.470 µs, but submission increased 72.594 → 80.957 µs. Low limits
lights/shadows rather than changing simulation. Browser Low additionally caps
framebuffer density at 1×; that resolution saving is not part of these 1× native
measurements. Headless software examples do not change quality workload.

All simulation step/pair/contact/CCD distributions match iteration 20 for all
fifteen examples. Native hashes/raw/gzip sizes, Wasm sizes/staged bytes and the
Low-run binaries were checked. The largest measured simulation/draw-recording
CPU p95 was 1,883.375 µs (Physics-3D); this excludes polling, submission, GPU
execution and presentation and is not an end-to-end frame-rate guarantee.

Local validation: 498 regular workspace tests, 14 Linux GLX/ALSA checks, strict
native/Windows GNU/macOS ARM64/Wasm Clippy and Rustdoc, fifteen native/Wasm release
builds and 360-frame native gameplay/PNG checks, all Node contracts, real-Wasm
style selection/persistence/quality overrides in every game, shader fallback,
multiplayer loss/reorder/background-client/native-gateway checks, a 640-tick
native recording verified in Wasm, and 21 Python checks. Final cave trimming
also passed its game tests, captures and actual-Wasm style/material checks.
Browser interaction reviewed Strike's Warm viewmodel/map and Crystal's Cool
spawn/reset, with no reported shader or lighting errors. All fifteen gallery
previews were refreshed; cave/Strike previews use early live frames rather than
a wall-facing route segment or the practice death overlay. Presets preserve HUD
colors and use the existing rendering paths. Details: [visual presets](VISUALS.md).
Windows/macOS remain cross-checks, not runtime verification. iOS SDK/device
checks remain excluded. No deployment command was issued.


### Local performance cleanup (2026-10-16)

This pass compares the local working tree containing the mobile detail settings,
Marble Run campaign and fullscreen changes against the same tree with the
performance edits below. Both reports share HEAD `f87eb6d`; that commit alone
**does not identify either measured source tree**. Per-game binary SHA-256 values
in the reports identify the executables actually run. All native and Wasm games
were rebuilt before recording artifact sizes. Nothing was pushed or deployed.

The shared Linux x86_64/Radeon 8060S machine used Rust 1.97.0, the release profile,
default game quality, native DPI scale 1, 60 warmup frames and 180 measured frames
in each of three processes per game. No builds or test suites ran during the
recordings. Background desktop workloads remained present, so sub-millisecond
tail changes require caution. These measurements are not phone or iOS results.
The independent player resolution setting was preserved; lighting quality no
longer imposes the historical browser Low 1x density cap described above.

Changes:

- Shared retained rendering extracts the shadow frustum once per environment
  update rather than normalizing six planes for every submitted mesh. Lighting
  and user detail changes immediately refresh it.
- The mesh registry keeps ordered IDs beside weak references. Host liveness and
  geometry lookups use binary search instead of scanning and upgrading every
  preceding mesh. Expired handles remain invalid and IDs never recycle.
- Software rasterization skips fog position interpolation when the entire
  triangle is inside the fog start sphere. Flat-colored triangles in this region
  reuse one color conversion. Gradient/fogged triangles retain perspective-correct
  interpolation, using one reciprocal for their attribute weights. Original
  depth division is retained to preserve coplanar surface ordering.
- Rotational physics reuses each contact's normal effective mass across velocity
  iterations after island waking. Tangent mass still follows current slip, and
  position corrections still recompute mass after rotations. Fixed/kinematic
  bodies skip zero-inertia quaternion transforms; 3D bodies skip the unused
  planar-angle calculation.
- Snapshot deltas select sparse/full representation before allocating, then write
  directly into one exactly sized output buffer. Packet bytes and bounds are
  unchanged. `ferravex-bench` now includes unchanged, sparse and dense snapshots.

| Local workload | Before mean | After mean | Observation |
|---|---:|---:|---|
| Courtyard software render, 960x540 | 14.367 ms | 11.844 ms | 17.6% less render time |
| 128-box rotational solver, sleeping enabled | 374.287 us | 304.119 us | 18.7% less step time |
| Same solver, sleep disabled | 2570.233 us | 2153.613 us | 16.2% less step time |
| Dense 1000-byte snapshot encoding | 1.39 us | 0.42 us | 69.8% less encode time |

The rigid workload uses 1200 ticks at 120 Hz and 12 velocity iterations. Its
constraint visits, persisted/warmed contact counts and final sleeping counts are
identical before/after. The snapshot timings use 8192 iterations after 256 warmup
iterations, exclude network transport and include allocation/free. Unchanged and
sparse timings vary by tenths of a microsecond between processes; this pass makes
no speedup claim for them or the unrelated FPS/translational-solver kernels.

A separate allocator probe ran both snapshot implementations in one process,
1000 encodes per case, with identical output bytes:

| Snapshot | Before allocations/reallocations | After | Before requested bytes | After |
|---|---:|---:|---:|---:|
| Unchanged 1000 bytes | 1 | 1 | 8 | 3 |
| Sparse (11 changes) | 6 | 1 | 164 | 36 |
| Dense (1000 changes) | 12 | 1 | 9195 | 1003 |

These are cumulative requested allocation bytes per encode, not live heap/peak
memory or wire sizes. The probe and original snapshot source are retained under
`target/perf/pass-reference` for local reproduction.

All fifteen final recordings preserve the exact distributions of draw calls,
vertices, upload bytes, physics steps, pairs, contacts and CCD impacts. GPU query
coverage is 100% for the thirteen GPU games. This is CPU-side optimization; no
GPU throughput improvement is claimed. The total of the fifteen Wasm modules grew
16,565 raw / 10,616 gzip bytes (0.27% / 0.45%); every per-artifact size/work budget
passes. This total excludes shared host files, docs and assets.

The final full comparison retained two tail flags: Dune Skimmer GPU p99 and
Physics 2D CPU p99. Those exact binary hashes had passed in the preceding full
recording. A same-configuration repeat of just those games against an unchanged
subset of the original baseline passed the original 20% / 200-us CPU / 20-us GPU
budgets. Original failure reports remain intact; no baseline or tolerance was
relaxed. This is evidence of shared-host variation, not certification of stable
frame pacing on mobile hardware.

A full 960x540 Courtyard framebuffer comparison changes 2074 of 2,073,600 color
channels by one quantization unit; none changes by more than one. Analytical
perspective-color/fog tests, clip/depth/winding tests, shadow-refresh tests and
registry pruning tests cover correctness. Validation passed 507 workspace tests,
14 native graphics/audio checks, all browser/Wasm integration suites, strict
native/Wasm Clippy, 21 Python tests, all fifteen native game captures and the
zero-external-Cargo-package audit. The complete local site also rebuilt successfully.

Raw local evidence (ignored, not committed):

- `target/perf/pass-before/report.json` and `pass-before-baseline.json`
- `target/perf/pass-after/report.json` (initial candidate, retained tail flags)
- `target/perf/pass-final/report.json` (intermediate candidate, passed budgets)
- `target/perf/pass-verified/report.json` (finished source, retained tail flags)
- `target/perf/pass-tail-repeat/report.json` (same-binary diagnostic repeat)
- `target/perf/pass-reference/{kernels-before,kernels-verified,net-alloc}.log`
- `target/perf/pass-reference/{rigid-before,rigid-final,rigid-awake-before,rigid-awake-final,pixel-comparison}.json`

Remaining candidates for a separately measured pass include the CCD ball sweep's
scan over target bodies and the ECS query's repeated type lookups. Existing
bounded navigation scratch, particle pools, audio voices, render instance buffers
and shared HUD buffers were left intact. Broader architectural changes need their
own representative workload rather than assuming they will improve these games.


## Adaptive rendering and playtest traces

Optional adaptive presentation and bounded local CSV diagnostics are described in
[shared quality update 20](QUALITY_SERIES.md#20-adaptive-presentation-and-local-playtest-reports).
Valid GPU timing can reduce browser density and shared shadows without changing
simulation ticks or saved fidelity choices. Native shared hosts adapt shadows;
CPU-only hosts cannot infer GPU pressure. Reports include real game/replay context
where supplied. Use Details > Export playtest CSV in the browser, or
`FERRAVEX_PLAYTEST=target/playtest.csv` for the shared native GPU host. These recent
event traces supplement the detailed performance harness above; they do not
replace controlled measurements or durable replay files.
