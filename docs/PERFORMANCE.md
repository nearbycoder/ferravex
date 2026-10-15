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
