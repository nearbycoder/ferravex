#!/usr/bin/env python3
"""Run first-party validation without installing tools or fetching dependencies.

Examples::

    python3 -B scripts/validate.py --mode fast
    python3 -B scripts/validate.py --mode full --group wasm --require-complete
    python3 -B scripts/validate.py --mode full --dry-run --json
    python3 -B scripts/validate.py --mode changed --changed-path web/loader.js --dry-run --json
    git diff --no-renames --name-only -z HEAD -- > /tmp/ferravex-paths.z
    python3 -B scripts/validate.py --mode changed --changed-paths-file /tmp/ferravex-paths.z --dry-run

See docs/VALIDATION_50.md for baseline scope and prerequisites; report additions
are documented below.
Changed mode selects conservative groups from explicit repository-relative paths;
it does not inspect Git, infer untracked changes, or establish full validation.
Supply both old and new names for renames (Git's --no-renames does this). Deleted
paths work without their files. --group adds coverage in changed mode. Full and
fast mode reject changed-path inputs, preserving their existing selection rules.
Unless --dry-run is set, changed mode executes full checks in the affected groups,
which can rebuild shared Wasm/native/site output. Coordinate those runs with the
integration owner when working in a shared checkout.

Reports use schema version 2: existing fields retain their meanings, and scope
adds path reasons and omitted check IDs. summary.complete still means all selected
checks passed; summary.full_complete requires a fresh, unfiltered full run. A
changed run remains advisory even if its selected groups cover the full plan.
No report, log or build artifact is reused as a passing check. Dry runs write
nothing and never count as complete, including an explicitly empty path list.
"""

import argparse
from collections import Counter
import ctypes.util
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import shutil
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parent.parent
GROUPS = ("rust", "python", "js", "wasm", "native", "site")
FAST_GROUPS = GROUPS[:3]
STATUSES = ("passed", "failed", "skipped", "planned")
SITE_TESTS = {"test_site.py", "test_search.py", "test_player_release.py"}
PYTHON_TESTS = {
    "test_audit_owned.py", "test_perf.py", "test_smoke_games.py",
    "test_ferravex_cli.py", "test_doctor.py", "test_new_game.py",
    "test_catalog.py", "test_validate.py",
}
# Explicit source-guide scope: link checks never compile or execute snippets.
DOC_CHECK_PATHS = (
    "docs/ENGINE_WORKFLOWS.md", "docs/RESPONSIVE_GAMES.md",
    "docs/QUICKSTART.md", "docs/AGENT_GUIDE.md", "docs/MIGRATION_50.md",
    "docs/VALIDATION_50.md", "docs/JOURNEY_200.md",
    "docs/MIGRATION_200.md", "docs/JOURNEY_200_GAMEPLAY.md",
    "docs/JOURNEY_200_PRESENTATION.md", "docs/JOURNEY_200_DEBUGGING.md",
    "docs/JOURNEY_200_PERFORMANCE.md", "docs/JOURNEY_200_ACCESSIBILITY.md",
    "docs/JOURNEY_200_TEMPLATES.md", "docs/PLAYER_GUIDE.md",
    "AGENTS.md", "CONTRIBUTING.md", "DEPENDENCIES.md", "README.md",
)
GUIDE_EXAMPLE_PATHS = (
    "docs/ENGINE_WORKFLOWS.md",
    "docs/QUICKSTART.md", "docs/MIGRATION_200.md", "docs/AGENT_GUIDE.md",
    "docs/JOURNEY_200_GAMEPLAY.md", "docs/JOURNEY_200_PRESENTATION.md",
    "docs/JOURNEY_200_DEBUGGING.md", "docs/JOURNEY_200_PERFORMANCE.md",
    "docs/JOURNEY_200_ACCESSIBILITY.md",
)
# These suites consume staged hosts or actual Wasm, even without "wasm-" in
# their names. Keep them out of fast mode; new wasm-*.mjs suites are automatic.
BUILT_WEB_TESTS = {
    "details.mjs", "gpu-timer.mjs", "marble-courses.mjs", "multiplayer.mjs",
    "quality-performance.mjs", "replays.mjs", "visuals.mjs",
    "wasm-animation.mjs", "wasm-audio.mjs", "wasm-content.mjs",
    "wasm-effects.mjs", "wasm-input.mjs", "wasm-materials.mjs",
    "wasm-saves.mjs", "wasm-ui.mjs",
}
# These suites also expose a source-only mode; the default still needs staging.
SOURCE_WEB_TESTS = {"details.mjs", "visuals.mjs"}


@dataclass(frozen=True)
class Check:
    """One command, its source inputs, environment needs and successful parents."""

    id: str
    group: str
    command: tuple[str, ...]
    sources: tuple[str, ...] = ()
    needs: tuple[str, ...] = ()
    requires: tuple[str, ...] = ()
    full_only: bool = False
    evidence: str = "source tests"
    smoke_report: str | None = None
    fresh_output: bool = False


def changed_path_groups(paths):
    """Return JSON-safe path/group/reason records, deduplicated in input order.

    Paths use repository-relative forward slashes. This is lexical classification,
    without filesystem checks: deleted files and unavailable build outputs cannot
    silently remove coverage. Unknown paths conservatively request every group::

        >>> impacts = changed_path_groups(["docs/INPUT.md", "./docs/INPUT.md"])
        >>> impacts[0]["groups"]
        ['python', 'site']
        >>> len(impacts)
        1
    """
    if isinstance(paths, (str, bytes)):
        raise ValueError("changed paths must be a sequence of repository-relative paths")
    impacts = {}
    for value in paths:
        if not isinstance(value, str) or not value or "\0" in value or "\\" in value:
            raise ValueError("changed paths must be nonempty repository-relative paths using '/' separators")
        path = PurePosixPath(value)
        if path.is_absolute() or PureWindowsPath(value).drive or ".." in path.parts or not path.parts:
            raise ValueError(f"changed path must stay inside the repository: {value!r}")
        name = path.as_posix()
        if name in impacts:
            continue
        fallback = False
        if (name in {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
                     "AGENTS.md", "DEPENDENCIES.md", "scripts/validate.py", "scripts/test_validate.py"}
                or path.parts[0] in {".cargo", ".github"}):
            groups, reason = GROUPS, "shared build, policy or validation configuration"
        elif path.parts[0] in {"crates", "examples"}:
            groups, reason = GROUPS, "workspace/catalog input affects downstream runtime and documentation checks"
        elif path.parts[:2] == ("web", "tests") and path.suffix == ".mjs":
            built = path.name in BUILT_WEB_TESTS or path.name.startswith("wasm-")
            groups = ("js", "wasm") if path.name in SOURCE_WEB_TESTS else (("wasm",) if built else ("js",))
            reason = ("source-only and staged-host/actual-Wasm tests" if path.name in SOURCE_WEB_TESTS
                      else ("staged-host/actual-Wasm tests" if built else "source-only host tests"))
        elif path.parts[0] == "web":
            groups, reason = ("python", "js", "wasm", "site"), "browser hosts are consumed by source, staged and site checks"
        elif path.parts[0] in {"docs", "website"} or (len(path.parts) == 1 and path.suffix == ".md"):
            groups, reason = ("python", "site"), "documentation/catalog and generated site checks"
        elif path.parts[0] == "templates":
            groups, reason = ("python",), "scaffolder and developer-tool regressions"
        elif path.parts[:1] == ("scripts",) and len(path.parts) == 2:
            if path.name in SITE_TESTS or path.name in {"build-site.py", "site_search.py", "serve-site.py"}:
                groups, reason = ("python", "site"), "site tooling and generated site checks"
            elif (path.name.startswith("test_") and path.suffix == ".py") or path.name in {
                "ferravex.py", "doctor.py", "new-game.py", "serve-web.py",
            }:
                groups, reason = ("python",), "developer-tool regressions"
            else:
                groups, reason, fallback = GROUPS, "unclassified script; conservatively select all groups", True
        else:
            groups, reason, fallback = GROUPS, "unclassified path; conservatively select all groups", True
        impacts[name] = {"path": name, "groups": list(groups), "reason": reason, "fallback": fallback}
    return list(impacts.values())


def read_changed_paths(path):
    """Read Git --name-only -z --no-renames output without losing unusual names.

    An empty file explicitly means no paths. Nonempty input must be NUL-terminated;
    newline-delimited input is rejected instead of silently planning wrong paths.
    """
    data = Path(path).read_bytes()
    if not data:
        return []
    if not data.endswith(b"\0"):
        raise ValueError("changed-paths file must be NUL-terminated (git diff --no-renames --name-only -z)")
    paths = [os.fsdecode(value) for value in data[:-1].split(b"\0")]
    # Validate here as well so malformed lists cannot become an implicit full run.
    return [impact["path"] for impact in changed_path_groups(paths)]


def _selection(mode, groups, changed_paths):
    if mode not in ("fast", "full", "changed"):
        raise ValueError("mode must be fast/full/changed")
    if set(groups or ()) - set(GROUPS):
        raise ValueError("unknown validation group")
    if mode == "changed":
        if changed_paths is None:
            raise ValueError("--mode changed requires --changed-path or --changed-paths-file (an empty file is allowed)")
        impacts = changed_path_groups(changed_paths)
        selected = set(groups or ()) | {group for impact in impacts for group in impact["groups"]}
        return tuple(group for group in GROUPS if group in selected), impacts
    if changed_paths is not None:
        raise ValueError("changed paths require --mode changed; full/fast coverage is never narrowed by paths")
    requested = tuple(dict.fromkeys(groups or (FAST_GROUPS if mode == "fast" else GROUPS)))
    if mode == "fast" and set(requested) - set(FAST_GROUPS):
        raise ValueError("wasm, native and site groups require --mode full or --mode changed")
    return requested, []


def build_plan(root=ROOT, *, mode="fast", groups=(), jobs=2, frames=180, changed_paths=None):
    """Return ordered checks, including each prerequisite exactly once.

    This function only reads directory entries. It never runs commands, probes
    tools or writes files, so it also supplies the CLI's dry-run plan. Changed
    mode uses full checks for the affected groups, with --group adding coverage::

        >>> checks = build_plan(mode="changed", changed_paths=["docs/INPUT.md"])
        >>> "wasm.build" in {check.id for check in checks}
        True
    """
    if jobs not in (1, 2) or frames < 1:
        raise ValueError("jobs must be 1/2, and frames positive")
    requested, _ = _selection(mode, groups, changed_paths)
    checks = []

    def add(id, group, command, **kwargs):
        checks.append(Check(id, group, tuple(command), **kwargs))

    def cargo(subcommand, *args):
        return ["cargo", subcommand, "--offline", "--locked", "--jobs", str(jobs), *args]

    def python(path, *args):
        return [sys.executable, "-B", path, *args]

    add("rust.owned", "rust", python("scripts/audit-owned.py"),
        sources=("scripts/audit-owned.py",), needs=("cargo",), evidence="dependency audit")
    add("rust.fmt", "rust", ["cargo", "fmt", "--all", "--", "--check"],
        needs=("cargo", "rustfmt"), evidence="format check only")
    for name, arguments in (
        ("check", ("--workspace", "--all-targets", "--all-features")),
        ("test", ("--workspace", "--all-features")),
        ("clippy", ("--workspace", "--all-targets", "--all-features", "--", "-D", "warnings")),
    ):
        add("rust." + name, "rust", cargo(name, *arguments),
            needs=("cargo", "clippy") if name == "clippy" else ("cargo",),
            requires=("rust.owned",), full_only=name == "clippy",
            evidence="Rust tests including rustdoc; ignored tests excluded" if name == "test" else "static check")
    guide_command = python("scripts/check-guide-examples.py", *GUIDE_EXAMPLE_PATHS)
    for package in ("ferravex", "ferravex-core", "ferravex-engine", "ferravex-play", "ferravex-math", "ferravex-render"):
        guide_command.extend(("-p", package))
    guide_command.extend(("--output", "{fresh_output}/results", "--target-dir", "{fresh_output}/cargo", "--json"))
    add("rust.guide-examples", "rust", guide_command,
        sources=("scripts/check-guide-examples.py", "scripts/check-docs.py", *GUIDE_EXAMPLE_PATHS),
        needs=("cargo", "rustc", "python3"), requires=("rust.owned",), full_only=True, fresh_output=True,
        evidence="explicit trusted Rust guide binaries compiled and executed on the native host; retained Cargo artifact evidence")

    add("python.catalog", "python", python("scripts/audit-catalog.py", "--json",
        "--migration" if mode == "fast" else "--require-session"),
        sources=("scripts/audit-catalog.py",), evidence="catalog/source audit and static session evidence; no runtime claim")
    add("python.docs", "python", python("scripts/check-docs.py", *DOC_CHECK_PATHS, "--json"),
        sources=("scripts/check-docs.py", *DOC_CHECK_PATHS),
        evidence="selected source-guide links and anchors; Rust examples are not compiled")
    names = PYTHON_TESTS | {p.name for p in (root / "scripts").glob("test_*.py")}
    for name in sorted(names - SITE_TESTS):
        path = "scripts/" + name
        add("python." + Path(name).stem, "python", python(path), sources=(path,))
    add("python.search-source", "python", python("scripts/test_search.py", "CorpusTests", "BrowserSearchTests"),
        sources=("scripts/test_search.py", "scripts/site_search.py"), needs=("node",),
        evidence="isolated corpus and generated search-module fixtures; no staged site")

    add("wasm.build", "wasm", python("scripts/build-web.py", "release"),
        sources=("scripts/build-web.py", "scripts/build-owned.py"),
        needs=("cargo", "python3", "wasm-target"), requires=("rust.owned",),
        full_only=True, evidence="all catalog Wasm modules and staged browser hosts")
    names = BUILT_WEB_TESTS | {"loader.mjs", "lifecycle.mjs"} | {
        p.name for p in (root / "web/tests").glob("*.mjs")
    }
    for name in sorted(names):
        built = name in BUILT_WEB_TESTS or name.startswith("wasm-")
        group = "wasm" if built else "js"
        path = "web/tests/" + name
        add(group + "." + Path(name).stem, group, ["node", path],
            sources=(path,), needs=("node",), requires=("wasm.build",) if built else (),
            full_only=built, evidence="Node with staged hosts/actual Wasm; no browser" if built else "Node host mocks; no browser")
        if name in SOURCE_WEB_TESTS:
            add("js." + Path(name).stem, "js", ["node", path, "--source-only"],
                sources=(path,), needs=("node",), evidence="Node source-only host mocks; no staged artifacts or browser")
    for name, path in (
        ("browser-input", "crates/ferravex-render/tests/browser-input.mjs"),
        ("platform-input", "examples/owned-orbit/tests/platform-input.mjs"),
        ("platform-session", "examples/owned-lab/tests/platform-session.mjs"),
    ):
        add("js." + name, "js", ["node", path], sources=(path,),
            needs=("node",), evidence="Node input mocks; no browser")
    add("wasm.server-build", "wasm", cargo("build", "--release", "-p", "ferravex-fps", "--bin", "ferravex-server"),
        needs=("cargo", "linux"), requires=("rust.owned",), full_only=True,
        evidence="native authority build for the Linux integration harness")
    add("wasm.multiplayer-native", "wasm", ["node", "web/tests/multiplayer.mjs", "--native"],
        sources=("web/tests/multiplayer.mjs",), needs=("node", "linux"),
        requires=("wasm.build", "wasm.server-build"), full_only=True,
        evidence="loopback native authority / Node WebSocket / actual Wasm")
    add("wasm.sizes", "wasm", python("scripts/report-size.py", "--profile", "release", "--json"),
        sources=("scripts/report-size.py",), needs=("cargo",), requires=("wasm.build",),
        full_only=True, evidence="artifact size inventory; missing artifacts remain labelled")
    add("wasm.templates", "wasm", python("scripts/test_template_parity.py", "--verify", "--web-profile", "release"),
        sources=("scripts/test_template_parity.py", "scripts/new-game.py"),
        needs=("cargo", "python3", "wasm-target"), requires=("rust.owned",),
        full_only=True, evidence="fresh 2D/3D scaffolds: native behavioral tests/linking and release Wasm export checks; no browser")

    add("native.build", "native", cargo("build", "--release", "--workspace", "--bins", "--all-features"),
        needs=("cargo",), requires=("rust.owned",), full_only=True,
        evidence="native compilation/linking; no runtime evidence")
    for name, output, games, needs in (
        ("headless", "target/game-smoke-headless", ("owned-lab", "owned-orbit"), ("cargo",)),
        ("smoke", "target/game-smoke", (), ("cargo", "linux-display")),
    ):
        command = python("scripts/smoke-games.py", "--frames", str(frames), "--output", output)
        for game in games:
            command.extend(("--game", game))
        add("native." + name, "native", command, sources=("scripts/smoke-games.py",),
            needs=needs, requires=("native.build",), full_only=True,
            evidence="software PNG export; no display/GPU" if games else "all catalog gameplay and PNG smoke; Linux GPU for windowed games",
            smoke_report=output + "/smoke-results.json")
    add("native.gpu", "native", cargo("test", "-p", "ferravex-render", "--lib", "--all-features", "native::gpu::tests::", "--", "--ignored", "--test-threads=1"),
        needs=("cargo", "linux-display"), requires=("rust.owned",), full_only=True,
        evidence="ignored Linux GPU pixel/readback/recovery tests; no audio/device test")

    add("site.docs", "site", cargo("doc", "--workspace", "--no-deps", "--all-features"),
        needs=("cargo",), requires=("rust.owned",), full_only=True, evidence="rustdoc build")
    add("site.build", "site", python("scripts/build-site.py", "--skip-build"),
        sources=("scripts/build-site.py",), needs=("cargo", "python3"),
        requires=("wasm.build", "site.docs"), full_only=True, evidence="generated documentation site")
    for name in sorted(SITE_TESTS):
        path = "scripts/" + name
        add("site." + Path(name).stem, "site", python(path), sources=(path,),
            needs=("node",) if name == "test_search.py" else (),
            requires=("site.build",), full_only=True, evidence="generated site/search/HTTP regression")
    add("site.search-browser-module", "site", ["node", "website/tests/search.mjs"],
        sources=("website/tests/search.mjs",), needs=("node",), requires=("site.build",),
        full_only=True, evidence="fresh generated search module and corpus in Node; no browser")

    index = {check.id: check for check in checks}
    selected = {}

    def include(check):
        if check.id in selected:
            return
        for dependency in check.requires:
            include(index[dependency])
        selected[check.id] = check

    for check in checks:
        if check.group in requested and (mode != "fast" or not check.full_only):
            include(check)
    return list(selected.values())


def child_environment(jobs=2):
    """Apply offline/job limits to Cargo calls made inside existing scripts too."""
    env = dict(os.environ)
    env.update(CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS=str(jobs),
               CARGO_TARGET_DIR=str(ROOT / "target"), PYTHONDONTWRITEBYTECODE="1")
    # Builders and actual-Wasm tests share the repository's fixed target paths.
    # Inheriting a cross target would also make a 'native' result misleading.
    env.pop("CARGO_BUILD_TARGET", None)
    return env


class Prerequisites:
    """Cache read-only tool/target/display probes; None means available."""

    def __init__(self, env):
        self.env = env
        self.cache = {}

    def missing(self, need):
        if need not in self.cache:
            self.cache[need] = self._probe(need)
        return self.cache[need]

    def _probe(self, need):
        if need == "linux":
            return None if sys.platform.startswith("linux") else "this integration harness requires Linux"
        if need == "linux-display":
            if self.missing("linux"):
                return "Linux X11/GPU checks are not implemented by this runner on this platform"
            if not self.env.get("DISPLAY", "").strip():
                return "DISPLAY is unset; use an existing X11 session or xvfb-run (nothing is installed automatically)"
            missing = [name for name in ("X11", "GL") if not ctypes.util.find_library(name)]
            return "missing system libraries: " + ", ".join(missing) if missing else None
        if need in ("rustfmt", "clippy"):
            return self._command_probe(["cargo", "fmt" if need == "rustfmt" else "clippy", "--version"], need)
        if need == "wasm-target":
            if not shutil.which("rustc", path=self.env.get("PATH")):
                return "rustc is missing; cannot check the installed wasm32-unknown-unknown standard library"
            try:
                result = subprocess.run(
                    ["rustc", "--print", "target-libdir", "--target", "wasm32-unknown-unknown"],
                    cwd=ROOT, env=self.env, capture_output=True, text=True, timeout=15,
                )
                if result.returncode == 0 and result.stdout.strip() and any(Path(result.stdout.strip()).glob("libstd-*.rlib")):
                    return None
            except (OSError, subprocess.TimeoutExpired) as error:
                return "cannot probe wasm32-unknown-unknown: " + str(error)
            return "wasm32-unknown-unknown standard library is unavailable; install it separately with rustup target add wasm32-unknown-unknown"
        return None if shutil.which(need, path=self.env.get("PATH")) else need + " is not on PATH"

    def _command_probe(self, command, name):
        try:
            result = subprocess.run(command, cwd=ROOT, env=self.env, capture_output=True, text=True, timeout=15)
        except (OSError, subprocess.TimeoutExpired) as error:
            return name + " is unavailable: " + str(error)
        if result.returncode:
            return name + " is unavailable: " + (result.stderr or result.stdout).strip()[-500:]
        return None


def aggregate(results, *, require_complete=False, dry_run=False):
    """Summarize selected checks only; an empty dry run is planned, not passed."""
    counts = Counter(row["status"] for row in results)
    if set(counts) - set(STATUSES):
        raise ValueError("unknown check status")
    status = ("failed" if counts["failed"] else "planned" if dry_run or counts["planned"]
              else "incomplete" if counts["skipped"] or not results else "passed")
    return {
        "status": status,
        "counts": {key: counts[key] for key in STATUSES},
        "complete": not dry_run and bool(results) and counts["passed"] == len(results),
        "exit_code": int(bool(counts["failed"] or (require_complete and status == "incomplete"))),
    }


def _smoke_outcome(path):
    """A zero exit must not turn an empty, stale or partially skipped smoke into PASS."""
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("smoke report must be a JSON object")
    rows = data.get("results")
    if not isinstance(rows, list) or not rows or any(not isinstance(row, dict) for row in rows):
        raise ValueError("smoke report must contain nonempty per-game results")
    def unavailable(row):
        return (row.get("status") == "skipped" or
                (row.get("status") == "blocked" and bool(row.get("environment_limitation"))))

    skipped = [row for row in rows if unavailable(row)]
    failed = [row for row in rows if not unavailable(row) and row.get("passed") is not True]
    if failed:
        return "failed", "smoke report contains unsuccessful game results", data
    if skipped:
        return "skipped", "smoke report skipped games; see per-game results", data
    return "passed", "", data


def run_checks(checks, *, root=ROOT, env=None, dry_run=False, timeout=1800,
               log_dir=None, prerequisites=None, run=subprocess.run, progress=None):
    """Execute a plan sequentially, preserving every failure and blocked result.

    ``run`` and ``prerequisites`` can be injected for regression tests. Missing
    source files are failures; absent tools and blocked dependencies are skips.
    Dry runs neither probe capabilities nor create reports/logs.
    """
    env = child_environment() if env is None else env
    prerequisites = Prerequisites(env) if prerequisites is None else prerequisites
    results = []
    by_id = {}
    for check in checks:
        row = {"id": check.id, "group": check.group, "command": list(check.command),
               "requires": list(check.requires), "evidence": check.evidence,
               "status": "planned", "reason": "dry run", "exit_code": None,
               "duration_seconds": 0.0}
        missing = [] if dry_run else [path for path in check.sources if not (root / path).is_file()]
        blocked = [name for name in check.requires if by_id.get(name, {}).get("status") != "passed"]
        if dry_run:
            pass
        elif missing:
            row.update(status="failed", reason="missing repository source: " + ", ".join(missing))
        elif blocked:
            row.update(status="skipped", reason="prerequisite did not pass: " + ", ".join(blocked))
        else:
            unavailable = [reason for need in check.needs if (reason := prerequisites.missing(need))]
            if unavailable:
                row.update(status="skipped", reason="; ".join(unavailable))
            else:
                if progress:
                    progress("RUN " + check.id)
                started = time.monotonic()
                output = ""
                try:
                    if check.fresh_output:
                        # Allocate only during execution. Each run keeps its own
                        # sources, binaries and evidence; no previous result is reused.
                        artifact_root = (log_dir or root / "target/validation-artifacts").resolve()
                        artifact_root.mkdir(parents=True, exist_ok=True)
                        directory = tempfile.mkdtemp(prefix=check.id + "-", dir=artifact_root)
                        row["artifact_directory"] = directory
                        row["command"] = [arg.replace("{fresh_output}", directory) for arg in check.command]
                    if check.smoke_report:
                        (root / check.smoke_report).unlink(missing_ok=True)
                    completed = run(tuple(row["command"]), cwd=root, env=env, capture_output=True,
                                    text=True, encoding="utf-8", errors="replace", timeout=timeout)
                    output = completed.stdout + completed.stderr
                    row.update(status="passed" if completed.returncode == 0 else "failed",
                               reason="" if completed.returncode == 0 else "command exited unsuccessfully",
                               exit_code=completed.returncode)
                    if check.smoke_report:
                        status, reason, details = _smoke_outcome(root / check.smoke_report)
                        row["details"] = details
                        # smoke-games exits 1 for both failures and explicitly
                        # blocked environments. Preserve that exit in the row,
                        # but distinguish missing coverage from failed gameplay.
                        if completed.returncode == 0 or (completed.returncode == 1 and status == "skipped"):
                            row.update(status=status, reason=reason)
                except subprocess.TimeoutExpired as error:
                    def decode(value):
                        return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""
                    output = decode(error.stdout) + decode(error.stderr)
                    row.update(status="failed", reason=f"command timed out after {timeout} seconds")
                except (OSError, ValueError) as error:
                    row.update(status="failed", reason=str(error))
                row["duration_seconds"] = round(time.monotonic() - started, 3)
                if log_dir is not None:
                    log_dir.mkdir(parents=True, exist_ok=True)
                    path = log_dir / (check.id + ".log")
                    path.write_text(output, encoding="utf-8")
                    row["log"] = str(path)
                if row["status"] == "failed":
                    row["output_tail"] = output[-6000:]
        results.append(row)
        by_id[check.id] = row
        if progress:
            progress(row["status"].upper() + " " + check.id + (": " + row["reason"] if row["reason"] else ""))
    return results


def main(argv=None):
    """CLI entry point; returns 0/1 for validation, argparse uses 2 for bad input."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=("fast", "full", "changed"), default="fast")
    parser.add_argument("--group", action="append", choices=GROUPS,
                        help="repeat to select groups (add groups in changed mode); prerequisites are included")
    parser.add_argument("--changed-path", action="append", metavar="PATH",
                        help="changed mode: repository-relative path, repeat for each edit/deletion/rename side")
    parser.add_argument("--changed-paths-file", type=Path, metavar="FILE",
                        help="changed mode: NUL-delimited paths from git diff --no-renames --name-only -z; empty is allowed")
    parser.add_argument("--jobs", type=int, choices=(1, 2), default=2)
    parser.add_argument("--frames", type=int, default=180, help="native smoke frames per game (default: 180)")
    parser.add_argument("--timeout", type=int, default=1800, help="timeout in seconds per command")
    parser.add_argument("--dry-run", action="store_true", help="list commands without probes, execution or file writes")
    parser.add_argument("--require-complete", action="store_true",
                        help="exit nonzero for skipped/empty selected coverage; does not require omitted groups")
    parser.add_argument("--json", action="store_true", help="emit one JSON document on stdout; progress goes to stderr")
    parser.add_argument("--report", type=Path, default=Path("target/validation/results.json"))
    args = parser.parse_args(argv)
    if args.frames < 1 or args.timeout < 1:
        parser.error("--frames and --timeout must be positive")
    try:
        changed_paths = args.changed_path
        if args.changed_paths_file is not None:
            changed_paths = [*(changed_paths or ()), *read_changed_paths(args.changed_paths_file)]
        requested, impacts = _selection(args.mode, args.group, changed_paths)
        checks = build_plan(mode=args.mode, groups=args.group, jobs=args.jobs, frames=args.frames,
                            changed_paths=changed_paths)
        full_checks = build_plan(mode="full", jobs=args.jobs, frames=args.frames)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    report_path = args.report if args.report.is_absolute() else ROOT / args.report
    selected_ids = {check.id for check in checks}
    full_ids = {check.id for check in full_checks}
    report = {
        "schema_version": 2, "mode": args.mode, "requested_groups": list(requested),
        "included_groups": sorted({check.group for check in checks}), "platform": sys.platform,
        "started_at": datetime.now(timezone.utc).isoformat(), "dry_run": args.dry_run,
        "policy": {"offline": True, "cargo_jobs": args.jobs, "require_complete": args.require_complete},
        "scope": {
            "selection": "changed" if args.mode == "changed" else "groups" if args.group else "mode",
            "changed_paths": impacts,
            "selected_check_count": len(checks), "full_check_count": len(full_checks),
            "omitted_checks": [check.id for check in full_checks if check.id not in selected_ids],
            "complete_means": "all selected checks passed in this run; see summary.full_complete for full coverage",
        },
        "limits": ["Node checks do not exercise a real browser, browser GPU or device.",
                   "Native smoke/pixel evidence applies only to the host used.",
                   "iOS CI is compile-only; this command does not exercise iOS devices or native audio."],
    }
    if args.mode == "changed":
        report["limits"].append(
            "Changed paths are caller-supplied, conservative group hints, not a complete dependency analysis. "
            "Even a successful changed run does not replace authoritative full validation."
        )
    try:
        report["results"] = run_checks(
            checks, env=child_environment(args.jobs), dry_run=args.dry_run, timeout=args.timeout,
            log_dir=None if args.dry_run else report_path.parent / (report_path.stem + "-logs"),
            progress=lambda message: print(message, file=sys.stderr, flush=True),
        )
        report["summary"] = aggregate(report["results"], require_complete=args.require_complete, dry_run=args.dry_run)
        report["summary"]["full_complete"] = bool(
            args.mode == "full" and report["summary"]["complete"] and selected_ids == full_ids
            and len(report["results"]) == len(full_checks)
            and {row["id"] for row in report["results"]} == full_ids
        )
        encoded = json.dumps(report, indent=2) + "\n"
        if not args.dry_run:
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(encoded, encoding="utf-8")
    except OSError as error:
        print("validation report/log error: " + str(error), file=sys.stderr)
        return 1
    if args.json:
        print(encoded, end="")
    else:
        summary = report["summary"]
        print(summary["status"].upper() + ": " + ", ".join(f"{n} {key}" for key, n in summary["counts"].items()))
        print("Coverage: " + ("full validation passed" if summary["full_complete"] else
              f"selected checks only; {len(report['scope']['omitted_checks'])} full-plan checks omitted; full_complete=false"))
        for impact in impacts:
            print(json.dumps(impact["path"]) + " -> " + ", ".join(impact["groups"]) + ": " + impact["reason"])
        if args.dry_run:
            for check in checks:
                print(check.id + ": " + json.dumps(check.command))
        else:
            print("Report: " + str(report_path))
    return report["summary"]["exit_code"]


if __name__ == "__main__":
    raise SystemExit(main())
