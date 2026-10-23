# Release-readiness review — stopping batch

Reviewed 2026-10-22 against mutable checkout based on HEAD
`7faf5c89eb86d8531ec08dfc7413fb8a0f4832c1`. Independent review, not a numbered
improvement. Only this report was written. No delegation, Cargo commands,
builds, staging, servers, browser work, remote operations, commits or deployments.

## Finding: P1 — fresh site builds reject their own new shared asset

**Location:** `scripts/build-site.py:281`, enforced at lines 286–287.
`scripts/build-owned.py:35–40,263–264` stages `status.js` unconditionally, but
the site's `allowed` set omits it. Per-game additions cannot allow this file:
they all have a host-directory prefix.

**Trigger/consequence:** a normal `python3 scripts/build-site.py` builds fresh
web/API output, then exits with
`Stale build inputs; run a full site build: play/status.js` before creating the
site. `--skip-build` rejects equivalent staged output too. `Dockerfile:7`
invokes the normal build, so this blocks the existing Railway Docker release
even when all Rust/Wasm compilation succeeds. Repeating the full build does not
repair the mismatch. This is independent of the repaired Python f-string issue.

**Cheap read-only reproduction, from repository root:**

```sh
python3 -B - <<'PY'
import ast
from pathlib import Path
site = ast.parse(Path('scripts/build-site.py').read_text())
owned = ast.parse(Path('scripts/build-owned.py').read_text())
main = next(n for n in site.body
            if isinstance(n, ast.FunctionDef) and n.name == 'main')
def assigned(nodes, name):
    return next(ast.literal_eval(n.value) for n in nodes
                if isinstance(n, ast.Assign)
                and any(isinstance(t, ast.Name) and t.id == name for t in n.targets))
print(sorted(set(assigned(owned.body, 'SHARED_ASSETS'))
             - assigned(main.body, 'allowed')))
PY
```

Observed output: `['status.js']`, exit 0. Also exercised the real imported
`build-site.py::main()` with argv `--skip-build`, mocked subprocesses/metadata,
successful required-input probes, and one staged `status.js` file. It produced
the exact stale-input SystemExit above. File creation/deletion methods were
guarded against execution; no actual subprocess or output mutation occurred.

**Suggested repair:** add `status.js` to the site allowlist. Add a narrow
source-only regression comparing the builder's shared assets to the site's
accepted assets so new staged assets cannot recreate this failure. Parent
should then run the real combined site build and deployment validation.

Reviewed hashes:

- `scripts/build-site.py`: `f50a6de1eef2ff93b623a9590978ba2caadbcca753f6636dc54c7dde69255956`
- `scripts/build-owned.py`: `8e1eb791121428cbe85c592addd701dc1fdd10d253ef7742c230ed55993133f7`

## Other evidence and review bounds

- Read AGENTS.md, CONTRIBUTING.md, DEPENDENCIES.md, the coding-agent guide,
  Journey400 ledger/contract, router232 receipt, and parent integration reports
  for status296 and projectile249. Existing owner validation is attributed to
  those records; this review did not rerun or independently establish it.
- Inspected facade/engine module exposure and representative new math,
  physics, net/FPS, play animation/camera and renderer UI/audio registrations.
  The inspected new APIs have production registration paths; no additional
  material registration defect was established. This is not exhaustive symbol
  coverage or a replacement for parent's feature-specific native/Wasm checks.
- `python3 -B scripts/build-site.py --check-sources`: PASS, 55 registered guide
  sources. `python3 -B scripts/build-site.py --self-test`: PASS, five source-only
  navigation/registration regressions. These do not exercise the failing
  post-build stale-asset guard.
- `python3 -B scripts/check-docs.py AGENTS.md CONTRIBUTING.md DEPENDENCIES.md
  README.md docs/AGENT_GUIDE.md docs/QUICKSTART.md docs/MIGRATION_200.md
  docs/JOURNEY_200.md docs/JOURNEY_400.md --json`: PASS, nine files, 348 local
  links, zero diagnostics. Twenty-two marked Rust examples remained
  `not_checked`; no Rust compiler was invoked.
- Parsed all 39 available `scripts/*.py` files with
  `ast.parse(..., feature_version=(3,11))`: PASS. In particular, `tile()` now
  computes search text before interpolation, and the six directly relevant
  site/build/audit/host scripts pass this grammar check. Interpreter was Python
  3.14.8; Python 3.11 is unavailable here. This establishes grammar
  compatibility only, not runtime execution under the Docker Python version.
- Inspected Dockerfile, railway.json, .dockerignore, static host and CI wiring.
  Docker builds the source tree using Rust 1.90/bookworm Python and copies only
  the generated site plus standalone stdlib server into Python 3.13 runtime.
  The host consumes PORT, defaults to 0.0.0.0 and provides /healthz, matching
  checked-in Railway settings. No demonstrated extra host/config blocker was
  found. Docker daemon and production service were not exercised; no current
  remote deployment or Rust 1.90 execution is claimed.
- Clone portability is still being finalized by parent. At the single archive
  inventory read, `docs/journey400` contained plan/tasks/contract/orchestration
  and proposals, but zero archived receipts; live target receipts numbered 83.
  Archived task data still contained target-relative and machine-absolute
  receipt/evidence references. These are **transient archive observations**,
  not a second final defect: parent explicitly owns the evolving archive.
  The checked Markdown entry-point links resolved at read time; receipt/evidence
  completeness in a fresh clone remains for parent to verify after copying.
- Task251 was explicitly still finalizing; no defect is inferred from that
  in-progress source or stale task counters. Root formatting is also parent
  work in progress. Neither was modified or polled.

One concrete deploy blocker found within this targeted scope. The entire large
combined diff was not audited. Final formatting, combined tests/Clippy,
feature-correct release artifacts, completed archive and deployment verification
remain parent's integration responsibility; existing browser/device limitations
are not expanded into new work by this review.
