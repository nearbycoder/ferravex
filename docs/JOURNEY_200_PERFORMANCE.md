# Journey200 performance evidence

This guide explains the implemented diagnostics and how to collect comparable
evidence. The results below were checked on 2026-10-22 against the working tree;
they establish scoped API behavior and allocation checks. No Journey200 game
timing baseline, fresh Wasm size baseline, sustained browser run or physical-device
measurement was collected for this guide. Parent integration and fresh staging
remain separate gates in [Journey200](JOURNEY_200.md).

Run commands from the repository root. Preserve the Journey50 baseline and
concurrent edits. The earlier [performance guide](PERFORMANCE.md) describes
benchmark boundaries; its historical timing results are not measurements of
these migrations. See [platform prerequisites](PLATFORMS.md) before interactive
measurement.

## Choose the quantity before choosing a budget

| Quantity and source | Interpretation and boundary |
| --- | --- |
| [`FrameStats`](../crates/ferravex-core/src/lib.rs): `sample_count`, `mean_ms`, `max_ms`, `percentile_ms`, `jitter_ms` | Caller-supplied elapsed seconds, summarized in milliseconds. A fixed simulation `dt` is not measured CPU/frame time. This bounded window does not read a clock. |
| [`WorkBudget::diagnostics`](../crates/ferravex-engine/src/work_budget.rs) | Caller-defined work units: limit, remaining, net consumed, denied requests/units and cost overflows. Units do not imply milliseconds. |
| [`RenderBudget::state`](../crates/ferravex-play/src/performance.rs) | Presentation controller reason, GPU filter, warm-up/evidence/cooldown counters. Its FPS setting is a target, not achieved FPS. |
| [`VisibilityQuery::stats`](../crates/ferravex-render/src/visibility.rs) | Tested/visible/frustum-culled/distance-culled counts, invalid transforms and visible-ID overflow. Visible includes IDs omitted from a full output buffer; overflow is not culling. |
| [`retained::telemetry_snapshot`](../crates/ferravex-render/src/retained.rs) | Calling-thread live meshes, expanded geometry bytes, current instances/batches/joints, limits, CPU buffer capacities and rejection categories. Recorded batches are not measured driver draw calls. |
| [`telemetry::FrameSample`](../crates/ferravex-core/src/telemetry.rs) | Native per-frame timing/work/allocation data consumed by `perf-games.py`. Unsupported optional metrics remain null. |

Retained `reset_telemetry()` returns and clears rejection counters only; it does
not free buffers, reset mesh identities or clear recorded geometry. Rejections
survive frame clears and saturate at `u64::MAX`. Snapshot inspection scans the
bounded mesh registry and does not finalize draws. Active and scratch capacities
are both relevant because finalization swaps buffers. Mesh payload bytes exclude
metadata and GPU copies; capacities are element counts, not byte counts or RSS.

The [native renderer runner](../crates/ferravex-render/src/runner.rs) measures
`cpu_us` before scene submission; `submit_us` covers issuing driver work and
swapping, including possible presentation blocking. `gpu_us` comes from tagged
driver queries and can be absent or delayed. Do not add overlapping CPU/GPU
durations and call the result frame latency. The
[Orbit host](../examples/owned-orbit/src/main.rs) and
[Courtyard host](../examples/owned-lab/src/main.rs) separately time their software
renderers. These hosts do not measure WebGL or Canvas2D performance. Screenshot
encoding and report serialization occur outside recorded frames.

Keep elapsed presentation measurements outside authoritative simulation. Consume
gameplay input edges on actual fixed ticks, count executed work and inspect
`FrameStep::dropped_seconds` when catch-up is limited. Reducing presentation
quality or opening a local network/replay menu must not stop authoritative ticks.
Do not claim an optimization if it merely skips required simulation or feedback
from accepted events. See the [agent guide](AGENT_GUIDE.md).

## Frame percentiles and jitter

`FrameStats<N>` retains the last `N` accepted samples. Zero is valid; negative,
non-finite and millisecond-overflowing inputs are rejected by `try_record` without
eviction. `record` silently delegates to it. Capacity zero is disabled, and
`clear` restarts the window. Filling a window is distinct from warming a renderer:
inspect the accepted count, and decide separately which startup measurements to
exclude from a steady-state experiment.

Percentiles use nearest rank: sort the retained values, select one-based rank
`ceil(p * n / 100)`, and select the minimum for `p = 0`. Empty windows or invalid
percentages return `None`. With four samples, p95 and p99 both select the maximum;
that does not establish a well-sampled tail. Queries copy the fixed array and
sort its populated prefix; they use `4 * N` bytes of stack scratch and work
proportional to copying `N` plus sorting the retained samples. Recording is
constant-time with no heap allocation. Query statistics at a suitable diagnostic
cadence rather than treating their cost as free.

`jitter_ms()` is population standard deviation, `sqrt(sum((x - mean)^2) / n)`.
It is independent of sample order, not a consecutive-frame difference. Empty,
singleton and constant windows return zero; check count before interpreting zero
as stability. Mean and jitter accumulate using `f64` and return `f32`.

This complete executable uses synthetic input, not device timing:

```rust,check,crate-type=bin
use ferravex_core::FrameStats;

fn main() {
    let mut stats = FrameStats::<120>::default();
    assert_eq!(stats.percentile_ms(99.0), None);
    for seconds in [0.010, 0.020, 0.030, 0.040] {
        assert!(stats.try_record(seconds));
    }
    assert!(!stats.try_record(f32::NAN));
    assert!(!stats.try_record(f32::MAX));
    assert_eq!(stats.sample_count(), 4);
    assert_eq!(stats.percentile_ms(50.0), Some(20.0));
    assert_eq!(stats.percentile_ms(95.0), Some(40.0));
    assert_eq!(stats.percentile_ms(99.0), Some(40.0));
    assert!((stats.jitter_ms() - 125.0_f32.sqrt()).abs() < 0.0001);
    println!("n={} mean_ms={} p99_ms={} jitter_ms={:.5}",
        stats.sample_count(), stats.mean_ms(),
        stats.percentile_ms(99.0).unwrap(), stats.jitter_ms());
    stats.clear();
    assert_eq!(stats.sample_count(), 0);
}
```

Observed output: `n=4 mean_ms=25 p99_ms=40 jitter_ms=11.18034`.

## Explain rejected work and presentation changes

`WorkBudget::try_reserve(items, cost_per_item)` prepays the checked product,
atomically, in call order. Overflow and insufficient allowance return distinct
`WorkBudgetError` variants. `refund(units)` restores unused current-period spend;
there is no reservation token or automatic cancellation. Callers own refund
correctness. `consumed` is net spend after refunds. Partial `grant_up_to` denial
counts only the ungranted remainder, whereas atomic denial counts the whole
representable request. Unrepresentable cost increments `cost_overflows` and
`denied_requests`, excluding that cost from `denied_units`. Diagnostic saturation
sets `saturated` without changing admission arithmetic. `reset` and `set_limit`
start a fresh accounting period.

This executable checks arithmetic and error accounting, not a CPU-time budget:

```rust,check,crate-type=bin
use ferravex_engine::work_budget::{WorkBudget, WorkBudgetError};

fn main() -> Result<(), WorkBudgetError> {
    let mut budget = WorkBudget::new(100);
    assert_eq!(budget.try_reserve(24, 3)?, 72);
    budget.refund(12)?;
    assert_eq!(budget.try_reserve(14, 3),
        Err(WorkBudgetError::Exhausted { requested: 42, remaining: 40 }));
    assert!(matches!(budget.try_reserve(u64::MAX, 2),
        Err(WorkBudgetError::CostOverflow { .. })));
    let report = budget.diagnostics();
    assert_eq!((report.consumed, report.remaining), (60, 40));
    assert_eq!((report.denied_requests, report.denied_units, report.cost_overflows),
        (2, 42, 1));
    println!("consumed={} remaining={} denied_requests={} denied_units={} overflows={}",
        report.consumed, report.remaining, report.denied_requests,
        report.denied_units, report.cost_overflows);
    budget.reset();
    assert_eq!(budget.remaining(), 100);
    assert_eq!(budget.diagnostics().denied_requests, 0);
    Ok(())
}
```

Observed output:
`consumed=60 remaining=40 denied_requests=2 denied_units=42 overflows=1`.

`RenderBudget` starts disabled. Once enabled, eight eligible GPU observations
seed its exponential filter. Both current and smoothed GPU time must exceed
110% of the target frame budget to add overload evidence. Thirty points lower
quality one level; nonqualifying observations subtract two points. Recovery
requires 180 consecutive observations below 65% on both measures. Changes impose
a 120-eligible-observation cooldown; scale stays between 1.0 and 0.5 in 0.125
steps. These are controller policy constants, not universal performance budgets.

Pause, frames longer than 250 ms, CPU time above 85% of target, or missing GPU
timing clear measurement/evidence history while retaining scale and freezing
cooldown. Invalid data leaves state unchanged. `state().reason` explains holds
and changes; `reset()` restores full scale and fresh warm-up while preserving
target and enabled setting. None of this changes simulation `dt` or saved user
fidelity preferences. Log the actual effective quality alongside timings so
quality reduction cannot masquerade as a speedup at equal fidelity.

This deterministic executable injects GPU values; it performs no GPU work:

```rust,check,crate-type=bin
use ferravex_play::performance::{BudgetReason, BudgetSample, RenderBudget};

fn main() {
    let mut budget = RenderBudget::new(60.0).unwrap();
    budget.set_enabled(true);
    let sample = BudgetSample {
        frame_ms: 35.0, cpu_ms: 2.0, gpu_ms: Some(30.0), paused: false,
    };
    for _ in 0..38 {
        budget.observe(sample).unwrap();
    }
    assert_eq!(budget.level(), 1);
    assert_eq!(budget.scale(), 0.875);
    assert_eq!(budget.state().reason, BudgetReason::LoweredQuality);
    println!("observations=38 level={} scale={} reason={:?}",
        budget.level(), budget.scale(), budget.state().reason);
    let before = budget.state();
    assert!(budget.observe(BudgetSample { cpu_ms: f32::NAN, ..sample }).is_err());
    assert_eq!(budget.state(), before);
    budget.reset();
    assert_eq!(budget.state().warmup_remaining, 8);
    assert_eq!(budget.scale(), 1.0);
}
```

Observed output: `observations=38 level=1 scale=0.875 reason=LoweredQuality`.

## Reproduce a baseline comparison

The implemented [performance runner](../scripts/perf-games.py) defaults to three
fresh processes, each discarding 60 warm-up frames and retaining 180 measured
frames. It uses the existing fixed-interval catalog workloads, not wall-clock
interactive gameplay. `--game` is repeatable; omitting it selects all games.
Builds use `--offline --locked -j 2`. During concurrent Journey200 work, coordinate
actual performance collection and builds with the parent; contention makes timing
comparisons noisy even when commands are correct.

The following is a **collection recipe, not an executed Journey200 baseline**.
It selects Orbit's headless software workload. Use a new experiment directory;
review the first report before approving it, then run the second command against
the intended candidate source under the same conditions:

```sh
python3 -B scripts/perf-games.py --game owned-orbit --profile release \
  --frames 180 --warmup 60 --runs 3 --quality default --min-samples 100 \
  --output target/perf/j200-orbit-before \
  --write-baseline target/perf/j200-orbit-baseline.json
python3 -B scripts/perf-games.py --game owned-orbit --profile release \
  --frames 180 --warmup 60 --runs 3 --quality default --min-samples 100 \
  --output target/perf/j200-orbit-after \
  --baseline target/perf/j200-orbit-baseline.json
```

`--write-baseline` deliberately creates/replaces the named file; it is not an
automatic acceptance decision. `--baseline` does not rewrite it. `--no-build`
uses existing binaries and transfers freshness responsibility to the caller.
The runner does not build Wasm; a Wasm file it happens to find can be stale.
Record artifact provenance separately before interpreting those size fields.

Each invocation preserves raw JSON under a fresh `recording-*` directory and
writes `report.json` under `--output`. Archive the raw files named by
`games[game].repetitions[*].raw_file` with the reports: paths are absolute, and
moving a baseline JSON alone does not move its observations. Raw SHA-256 hashes,
binary hash, baseline hash/revision, counts, coverage, per-run summaries and
noise are recorded. Report schema remains 2; `analysis` and `comparison` metadata
use schema 1. Older schema-2 reports can lack noise; missing noise stays null.

For a reproducible comparison, record source revision **and dirty working-tree
state**, workload, toolchain, features/build settings, backend, dimensions,
quality, CPU/GPU/driver, power state and other system load. The runner records
HEAD and binary hashes but does not capture the working-tree diff, CPU model,
driver version, every environment variable or power state. Its machine/config/
Rust-version/game-coverage checks, and per-game backend/dimensions/GPU-source/
quality checks, catch some mismatches; a compatible report is not proof of
identical experimental conditions. Repeat a control run to assess noise; preserve
outliers and raw samples rather than selecting the fastest run.

The runner reports mean, p50, p95, p99 and max using every measured value.
`noise.sample_stdev` uses **n − 1**, unlike `FrameStats::jitter_ms`.
`median_absolute_deviation` is the unscaled median distance from the median.
`repetition_noise` applies these estimators to per-run means, with measured/total
run counts and the min/max means. One observation has null sample standard
deviation. Frames can be correlated; these are descriptive statistics, not
confidence intervals. Noise never silently increases an allowance.

Both reports need at least `--min-samples` observations per measured metric
(default 100; configurable 2–163840). This is a p99 resolution floor, not a
confidence guarantee. Non-GPU metrics need full coverage. GPU coverage must be
at least 50% on both sides and may not drop by more than ten percentage points.
Unsupported metrics on both sides remain `not_measured`; changed availability
is incompatible. A null GPU duration is not zero GPU cost.

Eligible mean/p95/p99 comparisons use `current <= baseline + allowance`:

| Metric | Default allowance in `perf-games.py` |
| --- | --- |
| CPU, submission, software render time | Larger of 200 µs and 20% of baseline |
| GPU time | Larger of 20 µs and 20% of baseline |
| Work/allocation counters | Larger of 1 unit and 5% of baseline |
| Artifact raw/gzip bytes | Larger of 1024 bytes and 5% of baseline |

Timing and artifact tolerances are configurable; the emitted comparison policy
records applied values. These local regression allowances are not frame-rate
targets. In particular, the one-unit work floor can permit allocation growth
from zero: a strict zero-allocation requirement needs its dedicated regression
test, not just a passing aggregate report.

Overall outcomes are `not_compared` without a baseline, otherwise `incompatible`,
`regression`, `insufficient_samples`, or `passed`, in that precedence order.
Inspect individual checks: a regression in one metric can coexist with insufficient
GPU observations. CLI comparisons exit 1 for those three failure outcomes and
print distinct uppercase diagnostics. Insufficient evidence is not a pass or a
measured slowdown. Values, counts and reasons remain available; percentage delta
from a zero baseline is null.

## Keep artifact budgets separate

[`report-size.py`](../scripts/report-size.py) measures actual raw bytes and
deterministic gzip-9 bytes without building or proving freshness. Its schema-1
baseline comparison defaults to **zero growth**, unlike `perf-games.py`'s size
allowance. It requires matching measured artifact coverage, profile and
compression settings; native comparisons additionally check target/host context.
`--artifact` selects exact labels, `--require-complete` rejects missing selected
candidates, and `--budgets` supplies per-artifact absolute or growth limits.
Growth limits require a baseline. All configured limits apply independently.

After a parent-verified fresh release Wasm build, this **unexecuted measurement
recipe** records/checks one exact artifact (the destination directory must exist):

```sh
python3 -B scripts/report-size.py --profile release \
  --artifact wasm/emberline.wasm --require-complete --json \
  > target/perf/j200-emberline-size-baseline.json
python3 -B scripts/report-size.py --profile release \
  --artifact wasm/emberline.wasm --require-complete \
  --baseline target/perf/j200-emberline-size-baseline.json --json
```

Check exit status before accepting the baseline: JSON can be emitted on failure.
Missing measurements are never zero. Gzip bytes estimate transfer size, not
runtime memory. A single Wasm artifact excludes its shared host JavaScript,
textures/audio and other required payloads; define a complete delivery scope
before claiming download size. A `small` build also needs separate timing
measurements before making a throughput claim.

## What the warmed-allocation checks establish

The native `TrackingAllocator` counts successful Rust allocation/reallocation
calls and requested bytes while recording. Reallocation counts its new requested
size, not just growth. Deallocation does not subtract from those totals. They
are not live heap size, C/driver allocations, browser heap, GPU memory or RSS.
Its counters are process-wide, so other allocating Rust threads can contribute.
The focused Rust tests below instead use thread-local counting scopes.

Warm the exact paths, resource sizes and alternating scratch buffers before
starting a steady-state counter. Exercise accepted operations, rejected inputs,
wraparound and resets inside the measured scope. Keep logging/serialization
outside it. If sizes grow beyond the warmed capacity or Wasm memory changes,
measure those transitions separately instead of asserting that all future
frames are allocation-free.

Fresh checks for this guide established:

- [`journey200_002`](../crates/ferravex-core/tests/journey200_002.rs) asserted
  **zero successful Rust heap allocation/reallocation calls** across construction
  and 1,200 iterations of recording, invalid-input rejection, statistics queries,
  disabled-window use and periodic clear. This includes initial warm-up.
- [`journey200_097`](../crates/ferravex-render/tests/journey200_097.rs) warmed four
  CPU recording frames, then asserted **zero successful Rust heap allocation/
  reallocation calls** across 128 frames of 16 alternating retained submissions,
  invalid draw/mesh rejection, repeated snapshots and telemetry resets. Mesh
  creation is outside the scope; no native window or real GPU submission runs.
- [`effects-buffers.mjs`](../web/tests/effects-buffers.mjs) reserved 256 instances,
  warmed the upload path, then exercised 600 frames with varying counts/pointers.
  Instrumented typed-array/DataView constructors and mocked GL buffer/store/
  program/VAO counters did not increase. Other tests cover capacity growth,
  memory replacement/growth and recovery. This is a Node fixture, not browser
  garbage-collection or driver-memory measurement, and does not count every
  possible JavaScript object allocation.

## Executed checks and reproducible examples

Environment: Linux x86_64 (`7.2.8-2-cachyos`), Rust/Cargo 1.96.1, Python 3.14.8,
Node v26.11.1. Rust checks used native default features in the development/test
profile. HEAD was `7faf5c89eb86d8531ec08dfc7413fb8a0f4832c1` with uncommitted
Journey50/Journey200 changes; that hash alone cannot reconstruct this tree.

These commands were executed successfully for this guide:

```sh
cargo test -p ferravex-core --offline --locked -j 2 --test journey200_002
cargo test -p ferravex-engine --lib work_budget --offline --locked -j 2
cargo test -p ferravex-play --lib performance::tests --offline --locked -j 2
cargo test -p ferravex-render --test journey200_097 --offline --locked -j 2
python3 -B scripts/test_perf.py
python3 -B scripts/test_report_size.py
python3 -B -m doctest scripts/perf-games.py scripts/report-size.py
node web/tests/effects-buffers.mjs
```

Results: respectively **12, 11, 11, 1, 20, 29 tests passed**; both Python doctest
modules passed; **15 Node tests passed**. The Python suites use temporary/mocked
fixtures, including sample-policy and baseline-comparison failures; they did not
benchmark games or measure current release artifacts. All three CLI `--help`
commands (`perf-games.py`, `report-size.py`, `check-docs.py`) were also executed
to verify the documented options.

All three complete Rust blocks above were compiled **and run**, including their
assertions and printed results. This stdlib-only command repeats the build,
link check and execution, using Cargo's actual artifact identities rather than
selecting an arbitrary cached feature build. It writes extracted examples and
executables to a temporary directory:

```sh
python3 -B - <<'PY'
import json
from pathlib import Path
import subprocess
import tempfile

names = {"ferravex_core", "ferravex_engine", "ferravex_play"}
build = subprocess.run([
    "cargo", "build", "-p", "ferravex-core", "-p", "ferravex-engine",
    "-p", "ferravex-play", "--lib", "--offline", "--locked", "-j", "2",
    "--message-format=json",
], check=True, capture_output=True, text=True)
artifacts = {}
for line in build.stdout.splitlines():
    row = json.loads(line)
    if row.get("reason") == "compiler-artifact" and row["target"]["name"] in names:
        for filename in row["filenames"]:
            if filename.endswith(".rmeta"):
                library = Path(filename).with_suffix(".rlib")
                assert library.is_file(), library
                artifacts[row["target"]["name"]] = library
assert set(artifacts) == names
externs = []
for name, library in sorted(artifacts.items()):
    externs.extend(["--extern", f"{name}={library}"])
with tempfile.TemporaryDirectory(prefix="ferravex-j200-182-") as temporary:
    extracted = Path(temporary) / "examples"
    subprocess.run([
        "python3", "-B", "scripts/check-docs.py", "docs/JOURNEY_200_PERFORMANCE.md",
        "--check-rust", "--extract", str(extracted), *externs,
    ], check=True)
    manifest = json.loads((extracted / "manifest.json").read_text())
    for example in manifest["examples"]:
        command = example["argv"][:]
        command[command.index("--emit=metadata")] = "--emit=link"
        executable = extracted / (example["id"] + ".bin")
        command[command.index("-o") + 1] = str(executable)
        subprocess.run(command, cwd=extracted, check=True)
        subprocess.run([str(executable)], check=True)
PY
```

The checker itself only emits metadata. The final loop explicitly links and runs
the selected executables. The dependency-directory paths let the checker supply
the necessary `-L dependency=...` search directory; unqualified top-level rlib
paths alone cannot resolve all transitive crates. The task receipt records the
execution results. Source links were checked with both site and GitHub anchor
conventions.

No workspace/Wasm build, shared site generation, native-window playtest, fresh
browser/GPU run, audio listening, cross-platform determinism test or sustained
device-performance experiment was performed by this documentation task. The
shared validator now registers this guide for link checks and full
`rust.guide-examples` execution with the required libraries. Parent verifies the
combined output. Passing the scoped checks above
does not mark Journey200 fully integrated.
