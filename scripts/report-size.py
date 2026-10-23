#!/usr/bin/env python3
"""Report actual artifact bytes and deterministic gzip-9 transfer estimates.

Record a baseline with the same profile and artifact selection used for checks::

    python3 -B scripts/report-size.py --artifact wasm/emberline.wasm --json > baseline.json
    python3 -B scripts/report-size.py --artifact wasm/emberline.wasm \
        --baseline baseline.json --budgets budgets.json --json

Budget files use exact artifact labels and nonnegative integer byte limits::

    {"schema": 1, "artifacts": {"wasm/emberline.wasm": {
        "bytes": 1048576, "gzip9_bytes": 262144,
        "max_growth_bytes": 4096, "max_growth_gzip9_bytes": 1024
    }}}

Every configured limit is enforced independently; equality passes. Growth limits
require --baseline. Baseline checks default to zero growth for both metrics and
require identical measured artifact coverage, profile and compression settings.
Native checks also require the same explicit target (or host OS/architecture).
Paths may differ between checkouts. Unversioned historical inventories cannot
serve as checked baselines; record a schema-1 report first.

Default inventory mode permits partial builds, retaining every missing label.
Explicit selections, baseline artifacts and budget entries are required; use
--require-complete to require all discovered candidates. Exit status is 0 for a
passing report, 1 for missing/read/size failures, and 2 for invalid configuration.
--json emits reports even when measurements/checks fail. Gzip is a transfer
estimate, not runtime memory usage. This tool does not build or prove freshness.
"""

import argparse
import json
from pathlib import Path
import platform
import stat
import subprocess
import sys
import zlib

ROOT = Path(__file__).resolve().parent.parent
GAMES = json.loads((ROOT / "examples" / "catalog.json").read_text())
EXAMPLES = tuple(game["id"] for game in GAMES)
SCHEMA = 1
MAX_JSON_BYTES = 4 * 1024 * 1024
MAX_ARTIFACTS = 4096
CHUNK_BYTES = 64 * 1024
METRICS = ("bytes", "gzip9_bytes")
GROWTH_FIELDS = {"bytes": "max_growth_bytes", "gzip9_bytes": "max_growth_gzip9_bytes"}
COMPRESSION = {
    "format": "gzip", "level": 9, "mtime": 0,
    "zlib_version": zlib.ZLIB_RUNTIME_VERSION,
}
SHARED_WEB = (
    "runtime.js", "lifecycle.js", "visuals.js", "details.js", "fullscreen.js",
    "audio.js", "audio-worklet.js", "audio-ring.js", "input.js", "storage.js",
    "input-overlay.js", "retained.js", "retained-shader.js", "effects.js",
    "effects-shader.js", "material-tools.js", "content-tools.js", "replay-tools.js",
    "multiplayer-tools.js",
)


def _label(value):
    if (not isinstance(value, str) or len(value) > 1024 or "\\" in value
            or any(ord(char) < 32 for char in value)
            or any(part in ("", ".", "..") for part in value.split("/"))
            or value.split("/", 1)[0] not in ("native", "wasm", "web")
            or "/" not in value):
        raise ValueError(f"invalid artifact label: {value!r}")
    return value


def _byte_count(value, context):
    if type(value) is not int or not 0 <= value <= (1 << 63) - 1:
        raise ValueError(f"{context}: expected an integer byte count in 0..2^63-1")
    return value


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _invalid_constant(value):
    raise ValueError(f"non-finite JSON value: {value}")


def _read_json(path):
    with path.open("rb") as source:
        data = source.read(MAX_JSON_BYTES + 1)
    if len(data) > MAX_JSON_BYTES:
        raise ValueError(f"{path}: JSON exceeds {MAX_JSON_BYTES} bytes")
    try:
        return json.loads(data, object_pairs_hook=_unique_object,
                          parse_constant=_invalid_constant)
    except (ValueError, RecursionError) as error:
        raise ValueError(f"{path}: invalid JSON: {error}") from error


def validate_budgets(document):
    """Return a checked artifact-to-limits mapping; raise ValueError on bad input.

    >>> validate_budgets({"schema": 1, "artifacts": {"wasm/a.wasm": {"bytes": 0}}})
    {'wasm/a.wasm': {'bytes': 0}}
    """
    if (not isinstance(document, dict) or set(document) != {"schema", "artifacts"}
            or type(document["schema"]) is not int or document["schema"] != SCHEMA):
        raise ValueError("budgets require schema 1 and an artifacts object")
    entries = document["artifacts"]
    if not isinstance(entries, dict) or not 1 <= len(entries) <= MAX_ARTIFACTS:
        raise ValueError(f"budgets require 1..{MAX_ARTIFACTS} artifact entries")
    allowed = set(METRICS) | set(GROWTH_FIELDS.values())
    checked = {}
    for label, limits in entries.items():
        _label(label)
        if not isinstance(limits, dict) or not limits or set(limits) - allowed:
            raise ValueError(f"{label}: budgets must use {', '.join(sorted(allowed))}")
        checked[label] = {
            key: _byte_count(value, f"{label}/{key}") for key, value in limits.items()
        }
    return checked


def _baseline_rows(document, profile, native_target, native_host):
    if (not isinstance(document, dict) or type(document.get("schema")) is not int
            or document["schema"] != SCHEMA):
        raise ValueError("baseline requires a schema-1 size report")
    for key, expected in (("profile", profile), ("compression", COMPRESSION)):
        if (document.get(key) != expected
                or (key == "compression" and any(
                    type(document[key][field]) is not type(value)
                    for field, value in COMPRESSION.items()))):
            raise ValueError(f"baseline {key} differs; record a compatible baseline")
    if (document.get("ok") is not True or document.get("errors") != []
            or document.get("failures") != []):
        raise ValueError("baseline must be a successful report without errors/failures")
    entries = document.get("artifacts")
    if not isinstance(entries, list) or not 1 <= len(entries) <= MAX_ARTIFACTS:
        raise ValueError(f"baseline requires 1..{MAX_ARTIFACTS} measured artifacts")
    rows = {}
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("baseline artifact must be an object")
        label = _label(entry.get("artifact"))
        if label in rows:
            raise ValueError(f"duplicate baseline artifact: {label}")
        rows[label] = {
            key: _byte_count(entry.get(key), f"baseline {label}/{key}")
            for key in METRICS
        }
    missing = document.get("missing")
    if not isinstance(missing, list) or len(missing) > MAX_ARTIFACTS:
        raise ValueError("baseline missing must be a bounded list of artifact labels")
    for label in missing:
        _label(label)
    if len(set(missing)) != len(missing) or set(missing) & rows.keys():
        raise ValueError("baseline missing labels duplicate or overlap measurements")
    if any(label.startswith("native/") for label in rows):
        if "native_target" not in document or document["native_target"] != native_target:
            raise ValueError("baseline native_target differs")
        if native_target is None and document.get("native_host") != native_host:
            raise ValueError("baseline native_host differs")
    return rows


def measure_artifact(path):
    """Return {'bytes': int, 'gzip9_bytes': int} using bounded streaming storage.

    Read/OSError failures propagate to the report collector. The gzip stream has
    no filename or timestamp and matches gzip.compress(data, 9, mtime=0) sizes
    under the same zlib version, including empty and multi-chunk files.

    >>> import tempfile
    >>> with tempfile.TemporaryDirectory() as directory:
    ...     path = Path(directory) / "empty.wasm"
    ...     _ = path.write_bytes(b"")
    ...     measure_artifact(path)
    {'bytes': 0, 'gzip9_bytes': 20}
    """
    compressor = zlib.compressobj(level=9, wbits=31)
    raw_size = compressed_size = 0
    with path.open("rb") as source:
        while chunk := source.read(CHUNK_BYTES):
            raw_size += len(chunk)
            compressed_size += len(compressor.compress(chunk))
    compressed_size += len(compressor.flush())
    return {"bytes": raw_size, "gzip9_bytes": compressed_size}


def _candidates(target, profile, native_target):
    native = (target / native_target if native_target else target) / profile
    wasm = target / "wasm32-unknown-unknown" / profile
    candidates = [(f"native/{name}", native / name)
                  for name in (*EXAMPLES, "ferravex-server")]
    for name in EXAMPLES:
        filename = name.replace("-", "_") + ".wasm"
        candidates.append((f"wasm/{filename}", wasm / filename))
    for game in GAMES:
        directory = ROOT / "dist/web" / game["host"]
        candidates.extend((f"web/{game['host']}/{path.name}", path)
                          for path in sorted(directory.glob("*"))
                          if path.is_file() and path.suffix != ".wasm")
    candidates.extend((f"web/{name}", ROOT / "dist/web" / name) for name in SHARED_WEB)
    return candidates


def _is_file(path):
    try:
        return stat.S_ISREG(path.stat().st_mode)
    except FileNotFoundError:
        return False


def collect_report(candidates, *, profile="release", native_target=None,
                   artifacts=(), baseline=None, budgets=None, require_complete=False):
    """Measure (label, Path) candidates and return a schema-1 JSON-ready report.

    ``baseline`` and ``budgets`` are parsed JSON documents, validated before I/O.
    ``artifacts`` selects exact labels; every selected/budget/baseline artifact
    must be measured. Baseline coverage must match exactly. Missing optional
    candidates only fail with ``require_complete`` or when nothing was measured.
    Invalid configuration raises ValueError; measurement/check failures produce
    ``ok=False`` with diagnostics. No artifact or baseline is written.

    >>> collect_report([])["ok"]
    False
    """
    if profile not in ("debug", "release", "small"):
        raise ValueError("profile must be debug, release or small")
    native_host = {"system": platform.system(), "machine": platform.machine()}
    old = (_baseline_rows(baseline, profile, native_target, native_host)
           if baseline is not None else None)
    limits = validate_budgets(budgets) if budgets is not None else {}
    if old is None and any(set(row) & set(GROWTH_FIELDS.values()) for row in limits.values()):
        raise ValueError("growth budgets require --baseline")
    paths = {}
    for label, path in candidates:
        _label(label)
        if label in paths:
            raise ValueError(f"duplicate artifact candidate: {label}")
        paths[label] = path
        if len(paths) > MAX_ARTIFACTS:
            raise ValueError(f"at most {MAX_ARTIFACTS} artifact candidates supported")
    selected = {_label(label) for label in artifacts}
    if len(selected) > MAX_ARTIFACTS:
        raise ValueError(f"at most {MAX_ARTIFACTS} selected artifacts supported")
    expected = selected if selected else set(paths)
    required = selected | set(limits) | set(old or {})
    if require_complete:
        required |= expected
    rows, missing, errors = [], [], []
    for label in sorted(expected):
        path = paths.get(label)
        try:
            if path is not None and label.startswith("native/") and not _is_file(path):
                executable = path.with_suffix(".exe")
                if _is_file(executable):
                    path = executable
            if path is None or not _is_file(path):
                missing.append(label)
                continue
            sizes = measure_artifact(path)
            display_path = path.relative_to(ROOT) if path.is_relative_to(ROOT) else path
            rows.append({"artifact": label, "path": str(display_path), **sizes})
        except OSError as error:
            errors.append({"artifact": label, "message": str(error)})
    measured = {row["artifact"]: row for row in rows}
    failures = [f"{error['artifact']}: cannot read artifact: {error['message']}"
                for error in errors]
    if not rows:
        failures.append(f"no built artifacts found for profile {profile}")
    for label in sorted(required - measured.keys()):
        failures.append(f"{label}: required artifact was not measured")
    comparison = None
    if old is not None:
        added = sorted(measured.keys() - old.keys())
        removed = sorted(old.keys() - measured.keys())
        comparison = {"added": added, "removed": removed, "artifacts": []}
        for label in added:
            failures.append(f"{label}: not measured in baseline; artifact coverage differs")
        for label in sorted(measured.keys() & old.keys()):
            deltas = {"artifact": label}
            for metric in METRICS:
                previous = old[label][metric]
                value = measured[label][metric]
                limit = previous + limits.get(label, {}).get(GROWTH_FIELDS[metric], 0)
                deltas[metric] = {"baseline": previous, "delta": value - previous,
                                  "limit": limit}
                if value > limit:
                    failures.append(f"{label}/{metric}: {value} > baseline limit {limit}")
            comparison["artifacts"].append(deltas)
    for label, budget in sorted(limits.items()):
        if label in measured:
            for metric in METRICS:
                if metric in budget and measured[label][metric] > budget[metric]:
                    failures.append(
                        f"{label}/{metric}: {measured[label][metric]} > budget {budget[metric]}"
                    )
    return {
        "schema": SCHEMA, "profile": profile, "native_target": native_target,
        "native_host": native_host, "compression": dict(COMPRESSION),
        "artifacts": rows, "missing": missing, "errors": errors,
        "comparison": comparison, "budgets": limits, "failures": failures,
        "ok": not failures,
    }


def main(argv=None):
    """Run the CLI; return 0/1 for measured reports, argparse exits 2 on bad inputs."""
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--profile", choices=("debug", "release", "small"), default="release")
    parser.add_argument("--target",
                        help="native cross-compilation target, if built with cargo --target")
    parser.add_argument("--json", action="store_true", help="print machine-readable results")
    parser.add_argument("--artifact", action="append", default=[], metavar="LABEL",
                        help="require an exact artifact label; repeat to select a subset")
    parser.add_argument("--baseline", type=Path,
                        help="compare against a successful schema-1 report")
    parser.add_argument("--budgets", type=Path, help="schema-1 per-artifact JSON budget file")
    parser.add_argument("--require-complete", action="store_true",
                        help="fail if any candidate in the selected scope is missing")
    args = parser.parse_args(argv)
    try:
        baseline = _read_json(args.baseline) if args.baseline else None
        budgets = _read_json(args.budgets) if args.budgets else None
        # Reject invalid policy before invoking Cargo or measuring large files.
        if args.baseline:
            _baseline_rows(baseline, args.profile, args.target,
                           {"system": platform.system(), "machine": platform.machine()})
        if args.budgets:
            limits = validate_budgets(budgets)
            if baseline is None and any(
                    set(row) & set(GROWTH_FIELDS.values()) for row in limits.values()):
                raise ValueError("growth budgets require --baseline")
        for label in args.artifact:
            _label(label)
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--offline", "--no-deps", "--locked", "--format-version", "1"],
            cwd=ROOT, text=True,
        ))
        target = Path(metadata["target_directory"])
        report = collect_report(
            _candidates(target, args.profile, args.target), profile=args.profile,
            native_target=args.target, artifacts=args.artifact, baseline=baseline,
            budgets=budgets, require_complete=args.require_complete,
        )
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.error(str(error))
    if args.json:
        print(json.dumps(report, indent=2, allow_nan=False))
    else:
        print(f"Profile: {args.profile}; exact bytes, gzip level 9 (mtime=0)")
        print(f"{'Artifact':30} {'File bytes':>14} {'Gzip bytes':>14}")
        for row in report["artifacts"]:
            print(f"{row['artifact']:30} {row['bytes']:14,d} {row['gzip9_bytes']:14,d}")
        if report["comparison"] is not None:
            print("Baseline deltas (file bytes / gzip bytes):")
            for row in report["comparison"]["artifacts"]:
                print(f"{row['artifact']}: {row['bytes']['delta']:+d} / "
                      f"{row['gzip9_bytes']['delta']:+d}")
        if report["missing"]:
            print("Not built: " + ", ".join(report["missing"]))
        print("Gzip values are transfer estimates, not executable size or frame-time measurements.")
    for failure in report["failures"]:
        print("FAIL " + failure, file=sys.stderr)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
