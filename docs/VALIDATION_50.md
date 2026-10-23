# Unified validation

`scripts/validate.py` is a Python standard-library runner for the repository's
existing Rust, Python and Node checks. It does not install tools, fetch packages
or publish artifacts. Run it from any directory; commands use the checkout root.
Use Python 3.11 or newer for the tools' standard-library TOML support.

```sh
# Default: source checks, without building dist or capturing gameplay.
python3 -B scripts/validate.py --mode fast

# Inspect every command without running tools, probing capabilities or writing files.
python3 -B scripts/validate.py --mode full --dry-run --json

# Complete local coverage where prerequisites are available.
python3 -B scripts/validate.py --mode full --require-complete \
  --report target/validation/full.json

# Targeted selections; repeat --group. Dependencies are included once.
python3 -B scripts/validate.py --group python --json
python3 -B scripts/validate.py --mode full --group wasm --group js
python3 -B scripts/validate.py --mode full --group site

# Plan affected groups from explicit paths; no commands or builds execute.
python3 -B scripts/validate.py --mode changed \
  --changed-path docs/INPUT.md --dry-run --json

# Keep deleted paths and both rename sides in a NUL-delimited change list.
git diff --no-renames --name-only -z HEAD -- > /tmp/ferravex-changed-paths.z
python3 -B scripts/validate.py --mode changed \
  --changed-paths-file /tmp/ferravex-changed-paths.z --dry-run --json
```

`fast` selects `rust`, `python` and `js`. It still runs all ordinary workspace
Rust tests, including rustdoc examples; it is not a changed-files heuristic.
`full` adds Clippy, release Wasm/native builds, actual-Wasm tests, documentation
generation and native smoke/GPU tests. The `wasm`, `native` and `site` groups
require full or changed mode. A complete result applies only to the requested scope and its
prerequisites, not to omitted groups or every supported device.

`changed` selects full checks for groups affected by the supplied paths, including
fresh build prerequisites. Unknown paths conservatively select all groups;
additional `--group` flags add coverage. Paths must be checkout-relative, and
untracked paths must be supplied explicitly. An empty list selects no checks:
its dry run is planned, and an actual empty run is incomplete. Without `--dry-run`,
changed mode may regenerate shared artifacts and needs the same coordination as
full mode. It always reports `summary.full_complete: false`.

| Group | Checks |
| --- | --- |
| `rust` | First-party dependency audit; read-only formatting; workspace/all-target/all-feature check; workspace/all-feature tests and rustdoc. Full mode also runs strict Clippy. |
| `python` | Catalog consistency/migration audit, selected source-guide links/anchors and independent `scripts/test_*.py` suites, including developer CLI, doctor, scaffold, catalog, smoke reporting and this runner. Full mode requires observed session use in every demo. |
| `js` | Source-only Node host mocks, including Details/visual preferences with `--source-only`, loader/lifecycle/catalog checks when present, renderer input and Orbit Breaker platform input. |
| `wasm` | Existing release browser builder; every staged-host/actual-Wasm Node suite; Linux authority build and loopback WebSocket integration; artifact size inventory. |
| `native` | Release workspace binaries; separate Courtyard/Orbit headless PNG exports; all-catalog Linux gameplay captures; ignored Linux GPU pixel/readback tests. |
| `site` | Rustdoc, existing site builder, generated site/HTTP/search/player-release Python regressions and Node checks of the shipped search module/corpus. Includes `wasm.build` and the dependency audit. |

New independent `test_*.py` files and `web/tests/*.mjs` are discovered in sorted
order. The known Journey 50 Python suites are required even if missing, so an
incomplete checkout cannot silently drop CLI/catalog coverage. New `wasm-*.mjs`
suites automatically require the browser build. A differently named suite that
starts using generated web artifacts must be added to `BUILT_WEB_TESTS` in
`validate.py`. The existing artifact consumers (`details`, `visuals`, `replays`,
`multiplayer`, `gpu-timer`, `marble-courses`, `quality-performance`) are already
classified. Generated-site Python suites are classified in `SITE_TESTS`.
Details and visual preferences also run in fast mode with `--source-only`; their
default artifact checks remain behind `wasm.build`. `python.search-source` runs
only isolated corpus/search fixtures; `site.search-browser-module` imports the
freshly generated module and corpus after `site.build`.
The catalog's `--migration` report is advisory in fast mode; full mode uses
`--require-session`. Both are static source evidence, not a claim that pause,
restart or authoritative network timing has been verified at runtime.

`python.docs` checks the explicit `DOC_CHECK_PATHS` guide scope in the runner.
Missing documents or broken local links/anchors fail the check. This command
does not compile examples. For an individual guide, `scripts/check-docs.py`
accepts `--check-rust` to type-check complete crates in explicitly marked
`rust,check` fences using caller-selected prebuilt first-party artifacts.
Ordinary Rust fences remain unselected; neither mode runs examples or shell
blocks. The shared scope includes the Journey200 migration, gameplay,
presentation, debugging, performance, accessibility and template guides.
Add future guides explicitly so missing files and broken links fail validation.

`--jobs` accepts 1 or 2 (default 2). Cargo build/check/test/doc/Clippy commands use
`--offline --locked`; the inherited `CARGO_NET_OFFLINE=true` and
`CARGO_BUILD_JOBS` also constrain Cargo calls inside existing scripts. Formatting
uses `cargo fmt --all -- --check` and changes no source. The runner sets
`CARGO_TARGET_DIR` to this checkout's `target` and removes `CARGO_BUILD_TARGET`
from its child environment: the builders and actual-Wasm suites use fixed paths,
and native coverage must execute host binaries. Existing toolchain configuration
must therefore select the host by default. Python bytecode writes are disabled.

Full mode intentionally regenerates `dist/web`, `dist/site`, Rustdoc and smoke
artifacts through the existing scripts. Do not run full mode concurrently with
another build/site/smoke run in the same checkout. Use dry-run or targeted source
tests while implementation agents share the tree. This task's implementation
validation does not constitute a full run; final execution belongs to the parent.

## Results and exit codes

Every selected command gets one result:

- `passed`: the command returned zero and, for gameplay smoke, its fresh report
  contains nonempty successful per-game results.
- `failed`: nonzero exit, timeout, launch/report error or missing repository source.
- `skipped`: an environment prerequisite is missing, a prerequisite check did not
  pass, or the smoke tool reports skipped/environment-blocked games. The reason
  remains in the report. A smoke exit of 1 is treated as skipped only when its
  report identifies missing coverage and contains no game failures; the original
  exit code remains available.
- `planned`: dry-run only. This is never counted as passed or complete.

Independent checks continue after failures. Consumers of a failed/skipped build
are skipped rather than run against old artifacts. Native smoke reports are
removed before launch, so a previous successful capture cannot mask a missing
new report. Their per-game results are embedded in the validation result.

Exit 1 means a check failed, a report/log could not be written, or
`--require-complete` rejected skipped coverage. Exit 2 is an argument error.
Without `--require-complete`, a run with skips and no failures exits 0 but its
summary is **incomplete**, never passed. CI uses `--require-complete`. Dry-run
exits 0 with a **planned** summary, including when strict completion is requested.

JSON is always written to `target/validation/results.json` unless `--report`
selects another path. Dry-run writes nothing, including a supplied report path.
`--json` additionally emits one JSON document on stdout; progress goes to stderr.
Per-command stdout/stderr is retained in a sibling `<report-stem>-logs` directory;
failed commands also include their last 6,000 output characters in JSON.
`--timeout` sets a positive per-command timeout (default 1,800 seconds).

The version 2 report retains selected-scope results and adds `scope`, which
records per-path selection reasons, selected/full check counts and omitted check
IDs. `summary.complete` means all selected checks passed in this invocation.
`summary.full_complete` additionally requires full mode and the entire full plan;
neither a dry run nor a changed-mode run can satisfy it. An abbreviated report:

```json
{
  "schema_version": 2,
  "mode": "full",
  "requested_groups": ["native"],
  "included_groups": ["native", "rust"],
  "platform": "linux",
  "dry_run": false,
  "policy": {"offline": true, "cargo_jobs": 2, "require_complete": false},
  "results": [
    {
      "id": "native.smoke",
      "group": "native",
      "command": ["python3", "-B", "scripts/smoke-games.py", "--frames", "180", "--output", "target/game-smoke"],
      "requires": ["native.build"],
      "evidence": "all catalog gameplay and PNG smoke; Linux GPU for windowed games",
      "status": "skipped",
      "reason": "DISPLAY is unset; use an existing X11 session or xvfb-run (nothing is installed automatically)",
      "exit_code": null,
      "duration_seconds": 0.0
    }
  ],
  "summary": {
    "status": "incomplete",
    "counts": {"passed": 0, "failed": 0, "skipped": 1, "planned": 0},
    "complete": false,
    "full_complete": false,
    "exit_code": 0
  }
}
```

This is an abbreviated example of a skipped result, not an execution receipt.
Real reports also include `scope`, a UTC start time and explicit coverage limits. Result
counts describe commands, not individual assertions: a successful Rust test
command still excludes ignored tests except the explicit native GPU invocation.

## Environment and CI coverage

Missing Cargo, Node, rustfmt/Clippy or the installed Wasm standard library is an
explicit skip. The Wasm probe checks for the target's actual `libstd` rather than
assuming `rustup` must manage every toolchain. Some existing build scripts invoke
`python3` internally, so that command must be available as well as the running
Python interpreter. Missing checked-in scripts are failures, not optional tools.

Linux windowed smoke and GPU tests require `DISPLAY` and system X11/GL libraries.
With an already installed Xvfb/Mesa environment:

```sh
LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a -s '-screen 0 1280x1024x24' \
  python3 -B scripts/validate.py --mode full --group native --require-complete
```

Presence of those prerequisites is only permission to try the checks. A bad
display, unavailable GL context, broken capture or assertion failure is a real
failure. Headless software captures and native compilation are labelled
separately and do not establish GPU or interactive input support. The runner
does not start a display itself. `--frames` changes native smoke duration and
defaults to the existing CI workload of 180 frames.

CI preserves native Linux/Windows/macOS checks and both
`aarch64-apple-ios` / `aarch64-apple-ios-sim` SDK compile checks. It now routes
native Rust/tooling and Linux gameplay through this command, uploads JSON/logs
even on failure, and runs all source-host/actual-Wasm/site suites in the web job.
An independent PR job uploads a changed-path dry-run plan; this advisory output
does not trim or gate the authoritative native, web or iOS checks.
Rust formatting and Clippy also run on Windows/macOS. The iOS jobs retain their
explicit cross-target Cargo check and two-job/offline policy; they do not run
simulators or sign apps. CI's existing toolchain/system-package provisioning is
separate from this offline validation command.

Node mocks and Node Wasm execution are not real-browser visual verification.
Linux software-rendered captures do not validate Windows/macOS native runtime,
audio devices, mobile devices or artistic quality. Those checks require separate
environment-specific evidence; no skipped runtime is described as passing.

## Runner API and regression tests

`Check` describes a command and its dependencies. `build_plan(root=ROOT, *,
mode="fast", groups=(), jobs=2, frames=180, changed_paths=None)` reads directory entries without
executing tools. `run_checks(checks, *, root=ROOT, env=None, dry_run=False,
timeout=1800, log_dir=None, prerequisites=None, run=subprocess.run,
progress=None)` executes it with injectable probes/commands. `aggregate(results,
*, require_complete=False, dry_run=False)` returns the selected-scope summary.
`changed_path_groups(paths)` explains path classification, and
`read_changed_paths(path)` reads the NUL-delimited input. The CLI adds full-plan
coverage information to the report.

```sh
python3 -B scripts/test_validate.py
```

The tests verify selection, prerequisite ordering, CLI/catalog discovery,
offline/job policy, source errors, missing tools/display/target, failure
propagation, continuation, timeouts, smoke-report freshness, JSON output and
strict completion without launching builds or native/browser hosts.
