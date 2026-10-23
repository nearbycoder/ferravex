#!/usr/bin/env python3
r"""Compile and execute explicitly selected, TRUSTED repository Rust examples.

Example (run from the checkout; --output must not exist)::

    python3 -B scripts/check-guide-examples.py docs/AGENT_GUIDE.md \
        -p ferravex-core -p ferravex-play --output target/guide-run --json

Reuses check-docs.py's extraction, link/marker diagnostics and source hashes.
Only complete rust,check,crate-type=bin fences run (commas or spaces accepted).
Library/default-crate-type examples are reported as skipped; ordinary Rust,
shell, ignore, no_run and compile_fail fences never run. No directory discovery,
implicit scope, dependency inference, shell execution or source wrapping occurs.
Invalid markers/links stop execution. A scope without runnable bins is a failure.

Select direct externs with repeated -p/--package (the six supported first-party
libraries only). Cargo builds those libraries and their dependencies with default
features, --offline --locked --jobs 2, using the rustc -vV native host explicitly.
CARGO_BUILD_TARGET cannot redirect this to Wasm. --target-dir overrides Cargo's
metadata target_directory (which honors CARGO_TARGET_DIR/config); relative CLI
paths are relative to the invocation directory. Fresh Cargo JSON records, package
identity and native output paths establish artifacts; no filesystem globs run.

--output is a NEW persistent directory: report.json, checker.json, extracted
sources/manifest, bounded log excerpts and executables. Commands record cwd,
argv, environment overrides, return codes and timeouts; source hashes identify
what ran. The extraction manifest retains the checker's compile-only argv; use
report.json for actual compile/run commands. JSON stdout is schema_version 1.
Exit 0 means every selected bin passed; 1 means validation/build/compile/runtime
failure; 2 means invalid options. Partial failures never count as execution.

check-docs.py remains compile-only, with its original disclaimer unchanged:
This is not a Rust sandbox: explicitly selected Rust can use compiler file-reading
facilities such as include_str!, and artifacts/toolchains must be trusted.
This wrapper additionally EXECUTES trusted code with the caller's permissions,
filesystem, environment and network access. Timeouts are not a security sandbox
or memory/disk quota. Temporary command logs may grow until the timeout; retained
logs/readbacks are capped at 2 MiB each. POSIX timeouts kill the command's process
group; other platforms kill the direct child only. No platform support claim.
"""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent
PACKAGES = ("ferravex", "ferravex-core", "ferravex-engine", "ferravex-play",
            "ferravex-math", "ferravex-render")
MAX_LOG_BYTES = 2 * 1024 * 1024
SPEC = importlib.util.spec_from_file_location("ferravex_check_docs", Path(__file__).with_name("check-docs.py"))
DOCS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DOCS)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_command(argv, *, cwd, log_prefix, timeout, env=None):
    """Run literal argv with a deadline; retain bounded logs and explicit outcome.

    No pipe waits on inherited descendant handles; POSIX timeouts kill the group.
    A timeout remains a failure even if the process exits during termination.
    """
    result = dict(argv=list(map(str, argv)), cwd=str(cwd), env=env or {},
                  timeout_seconds=timeout, returncode=None, status="not_started")
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        try:
            with subprocess.Popen(result["argv"], cwd=cwd, env=os.environ | (env or {}),
                                  stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                  shell=False, start_new_session=os.name == "posix") as child:
                try:
                    result["returncode"] = child.wait(timeout=timeout)
                    result["status"] = "passed" if child.returncode == 0 else "failed"
                except subprocess.TimeoutExpired:
                    if os.name == "posix":
                        try:
                            os.killpg(child.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    else:
                        child.kill()
                    result["returncode"] = child.wait()
                    result["status"] = "timeout"
        except OSError as error:
            result.update(status="spawn_failed", error=str(error))
        for label, stream in (("stdout", stdout), ("stderr", stderr)):
            stream.seek(0)
            data = stream.read(MAX_LOG_BYTES + 1)
            path = Path(str(log_prefix) + "." + label)
            path.write_bytes(data[:MAX_LOG_BYTES])
            result[label] = str(path)
            result[label + "_truncated"] = len(data) > MAX_LOG_BYTES
    return result


def read_log(command, stream="stdout"):
    return Path(command[stream]).read_text(encoding="utf-8", errors="replace")


def select_artifacts(records, packages, metadata, root, target_dir, host):
    """Resolve unique native rlibs from this successful Cargo JSON invocation.

    Match metadata workspace package IDs, source targets and the explicitly
    requested dev/host directory. Top-level rlibs need the sibling deps search
    directory too. Never guess a hashed filename from an rmeta or glob.
    """
    if not any(row.get("reason") == "build-finished" and row.get("success") is True
               for row in records):
        raise ValueError("Cargo did not report a successful build-finished record")
    native = (target_dir / host / "debug").resolve()
    deps = native / "deps"
    artifacts = {}
    for name in packages:
        expected_manifest = (root / "crates" / name / "Cargo.toml").resolve()
        candidates = [row for row in metadata["packages"] if row["name"] == name
                      and row["id"] in metadata["workspace_members"]
                      and row.get("source") is None
                      and Path(row["manifest_path"]).resolve() == expected_manifest]
        if len(candidates) != 1:
            raise ValueError(f"{name}: expected one first-party workspace package")
        package = candidates[0]
        targets = [row for row in package["targets"] if "lib" in row["kind"]
                   and "lib" in row["crate_types"]]
        if len(targets) != 1:
            raise ValueError(f"{name}: expected one ordinary library target")
        target = targets[0]
        matches = {}
        for record in records:
            built = record.get("target", {})
            if (record.get("reason") != "compiler-artifact"
                    or record.get("package_id") != package["id"]
                    or record.get("profile", {}).get("test") is not False
                    or built.get("name") != target["name"]
                    or built.get("kind") != target["kind"]
                    or built.get("crate_types") != target["crate_types"]
                    or built.get("src_path") != target["src_path"]):
                continue
            for filename in record.get("filenames", []):
                path = Path(filename).resolve()
                if path.suffix == ".rlib" and path.parent in {native, deps}:
                    matches[str(path)] = record
        if len(matches) != 1:
            raise ValueError(f"{name}: expected one native rlib from Cargo, found {len(matches)}")
        filename, record = next(iter(matches.items()))
        if not Path(filename).is_file() or not deps.is_dir():
            raise ValueError(f"{name}: reported rlib or matching dependency directory is missing")
        artifacts[target["name"]] = dict(path=filename, dependency_dir=str(deps),
                                        package=name, evidence=record)
    return artifacts


def compile_command(example, artifacts, rustc, host, binary):
    source = binary.parent / (example["id"] + ".rs")
    command = [rustc, str(source), "--crate-name", example["id"],
               "--edition=" + example["edition"], "--crate-type=bin", "--target", host,
               "--error-format=json", "-o", str(binary)]
    for name, artifact in sorted(artifacts.items()):
        command.extend(("--extern", name + "=" + artifact["path"]))
    for directory in sorted({row["dependency_dir"] for row in artifacts.values()}):
        command.extend(("-L", "dependency=" + directory))
    return command


def failure(report, code, message, example=None):
    example = example or {}
    report["diagnostics"].append(DOCS._diagnostic(
        example.get("path", ""), example.get("line", 0), code, message))


def compile_diagnostics(report, example, command):
    """Map rustc JSON primary spans through the checker's hidden-line offsets."""
    mapped = False
    for raw in read_log(command, "stderr").splitlines():
        try:
            error = json.loads(raw)
        except ValueError:
            continue
        if not isinstance(error, dict) or error.get("level") != "error":
            continue
        for span in error.get("spans", []):
            line, column = span.get("line_start"), span.get("column_start")
            if (span.get("is_primary") and span.get("file_name") == command["argv"][1]
                    and isinstance(line, int) and 1 <= line <= len(example["column_offsets"])
                    and isinstance(column, int)):
                report["diagnostics"].append(DOCS._diagnostic(
                    example["path"], example["code_line"] + line - 1, "rust-compile",
                    error.get("message", "Compilation failed"),
                    column=column + example["column_offsets"][line - 1]))
                mapped = True
                break
    if not mapped:
        failure(report, "rust-compile", f"{command['status']}; returncode={command['returncode']}; "
                f"see {command['stderr']}", example)


def validate(args, report):
    root, output = args.root, args.output
    for scope in args.paths:
        path = (root / scope).resolve()
        if (not path.is_relative_to(root) or not path.is_file()
                or path.suffix.lower() not in {".md", ".markdown"}):
            failure(report, "invalid-scope", f"Select explicit repository Markdown files: {scope}")
    if report["diagnostics"]:
        return
    checked = DOCS.check_docs(root, args.paths, anchor_style=args.anchor_style)
    write_json(output / "checker.json", checked)
    report["checker"] = str(output / "checker.json")
    report["diagnostics"].extend(checked["diagnostics"])
    report["files"] = checked["files"]
    bins = []
    for example in checked["examples"]:
        runnable = example["crate_type"] == "bin"  # The checker's default is lib, never bin.
        row = {key: example[key] for key in ("id", "path", "line", "edition", "crate_type", "sha256")}
        row.update(status="not_run" if runnable else "skipped_library", compile=None, run=None)
        report["examples"].append(row)
        if runnable:
            bins.append((example, row))
    if report["diagnostics"]:
        return
    if not bins:
        failure(report, "no-binaries", "No explicit rust,check,crate-type=bin examples in selected files")
        return
    extracted = output / "examples"
    DOCS.extract_examples({"examples": [example for example, _ in bins]}, extracted)
    for example, row in bins:
        row["source"] = str(extracted / (example["id"] + ".rs"))

    def command(stage, argv, timeout, env=None):
        result = run_command(argv, cwd=root, log_prefix=output / stage,
                             timeout=timeout, env=env)
        report["commands"][stage] = result
        if result["status"] != "passed":
            failure(report, stage + "-failed", f"{result['status']}; returncode={result['returncode']}; "
                    f"see {result['stderr']}")
            return None
        if result["stdout_truncated"]:
            failure(report, stage + "-output", "Output exceeded the parsing limit; see retained log")
            return None
        return read_log(result)

    rustc = shutil.which(args.rustc)
    if rustc is None:
        failure(report, "missing-rustc", f"Compiler not found: {args.rustc}")
        return
    rustc = str(Path(rustc).absolute())  # Do not resolve rustup's rustc symlink into rustup.
    version = command("rustc-version", [rustc, "-vV"], args.compile_timeout)
    if version is None:
        return
    hosts = re.findall(r"^host: ([A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)+)$", version, re.M)
    if len(hosts) != 1:
        failure(report, "invalid-host", "rustc -vV must report one native host triple")
        return
    host = hosts[0]
    report["host"] = host
    artifacts = {}
    if args.package:
        cargo = shutil.which(args.cargo)
        if cargo is None:
            failure(report, "missing-cargo", f"Cargo not found: {args.cargo}")
            return
        cargo = str(Path(cargo).absolute())
        if command("cargo-version", [cargo, "--version"], args.compile_timeout) is None:
            return
        # Ensure Cargo and the direct compiler use the same toolchain/host.
        env = {"RUSTC": rustc}
        manifest = str(root / "Cargo.toml")
        raw = command("metadata", [cargo, "metadata", "--no-deps", "--format-version=1",
                                  "--offline", "--locked", "--manifest-path", manifest],
                      args.build_timeout, env)
        if raw is None:
            return
        metadata = json.loads(raw)
        target_dir = args.target_dir or Path(metadata["target_directory"]).resolve()
        report["target_dir"] = str(target_dir)
        argv = [cargo, "build", "--manifest-path", manifest, "--lib", "--offline", "--locked",
                "--jobs", "2", "--profile", "dev", "--target", host, "--target-dir", str(target_dir),
                "--message-format=json"]
        for package in args.package:
            argv.extend(("--package", package))
        raw = command("build", argv, args.build_timeout, env)
        if raw is None:
            return
        try:
            records = [json.loads(line) for line in raw.splitlines() if line.strip()]
            if not all(isinstance(row, dict) for row in records):
                raise ValueError("Cargo JSON records must be objects")
            artifacts = select_artifacts(records, args.package, metadata, root, target_dir, host)
        except (KeyError, TypeError, ValueError) as error:
            failure(report, "artifact-selection", str(error))
            return
    report["artifacts"] = artifacts
    for example, row in bins:
        binary = extracted / (example["id"] + (".exe" if os.name == "nt" else ".bin"))
        # Keep the execution tied to the exact checker source, even in a shared checkout.
        if hashlib.sha256(Path(row["source"]).read_bytes()).hexdigest() != example["sha256"]:
            row["status"] = "source_changed"
            failure(report, "source-changed", "Extracted source hash changed before compilation", example)
            continue
        # Match Cargo's cwd for rustup directory/toolchain overrides, including
        # when --output is outside the checkout. All compiler paths are absolute.
        row["compile"] = run_command(
            compile_command(example, artifacts, rustc, host, binary), cwd=root,
            log_prefix=extracted / (example["id"] + "-compile"), timeout=args.compile_timeout)
        if row["compile"]["status"] != "passed":
            row["status"] = "compile_failed"
            compile_diagnostics(report, example, row["compile"])
            continue
        row["run"] = run_command([str(binary)], cwd=extracted,
                                 log_prefix=extracted / (example["id"] + "-run"), timeout=args.run_timeout)
        row["status"] = "passed" if row["run"]["status"] == "passed" else "run_failed"
        if row["status"] != "passed":
            failure(report, "rust-run", f"{row['run']['status']}; returncode={row['run']['returncode']}; "
                    f"see {row['run']['stderr']}", example)


def main(argv=None):
    """CLI; return 0 for full selected success, 1 for failures, 2 for usage."""
    argv = list(sys.argv[1:] if argv is None else argv)
    parser = DOCS._Parser(description=__doc__, allow_abbrev=False,
                          formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("paths", nargs="+", help="explicit trusted Markdown files relative to --root")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path, required=True, help="NEW report/extraction directory")
    parser.add_argument("-p", "--package", action="append", choices=PACKAGES, default=[],
                        help="direct first-party extern; repeat as needed; default: std only")
    parser.add_argument("--target-dir", type=Path, help="override Cargo target dir; relative to invocation cwd")
    parser.add_argument("--anchor-style", choices=("site", "github"), default="site")
    parser.add_argument("--rustc", default=os.environ.get("RUSTC", "rustc"))
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--build-timeout", type=float, default=300, help="seconds per Cargo command (max 900)")
    parser.add_argument("--compile-timeout", type=float, default=30, help="seconds per compiler command (max 300)")
    parser.add_argument("--run-timeout", type=float, default=10, help="seconds per example (max 60)")
    parser.add_argument("--json", action="store_true", help="emit only the structured result on stdout")
    report = dict(schema_version=1, ok=False, status="failed", files=[], examples=[], commands={},
                  artifacts={}, diagnostics=[], host=None)
    output = None
    exit_code = 0
    try:
        args = parser.parse_args(argv)
        for key, limit in (("build_timeout", 900), ("compile_timeout", 300), ("run_timeout", 60)):
            value = getattr(args, key)
            if not math.isfinite(value) or not 0 < value <= limit:
                parser.error(f"--{key.replace('_', '-')} must be finite and in (0, {limit}]")
        args.root = args.root.resolve()
        args.output = args.output.resolve()
        if args.target_dir:
            args.target_dir = args.target_dir.resolve()
        args.package = sorted(set(args.package))
        if args.output.exists():
            parser.error("--output must be a NEW directory; existing results are never overwritten")
        args.output.mkdir(parents=True, exist_ok=False)
        output = args.output
        report.update(root=str(args.root), output=str(output), paths=args.paths, packages=args.package,
                      timeouts=dict(build=args.build_timeout, compile=args.compile_timeout, run=args.run_timeout))
        try:
            validate(args, report)
        except (OSError, ValueError, KeyError, TypeError, RuntimeError) as error:
            failure(report, "validation-failed", f"{type(error).__name__}: {error}")
    except ValueError as error:
        failure(report, "usage", str(error))
        exit_code = 2
    except OSError as error:
        failure(report, "output-failed", str(error))
    bins = [row for row in report["examples"] if row["crate_type"] == "bin"]
    report["counts"] = dict(selected=len(bins), passed=sum(row["status"] == "passed" for row in bins),
                          compiled=sum(row["compile"] is not None and row["compile"]["status"] == "passed"
                                       for row in bins),
                          executed=sum(row["run"] is not None and row["run"]["status"] != "spawn_failed"
                                       for row in bins),
                          skipped_libraries=len(report["examples"]) - len(bins))
    report["ok"] = bool(bins) and report["counts"]["passed"] == len(bins) and not report["diagnostics"]
    report["status"] = "passed" if report["ok"] else "failed"
    report["exit_code"] = exit_code or (0 if report["ok"] else 1)
    if output:
        try:
            write_json(output / "report.json", report)
        except OSError as error:
            failure(report, "output-failed", str(error))
            report.update(ok=False, status="failed", exit_code=1)
    if "--json" in argv:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        for row in report["diagnostics"]:
            print(f"{row['path']}:{row['line']}:{row['column']}: {row['code']}: {row['message']}")
        counts = report["counts"]
        print(f"{counts['passed']}/{counts['selected']} trusted binaries passed; "
              f"{counts['compiled']} compiled, {counts['executed']} executed; report: {output}")
    return report["exit_code"]


if __name__ == "__main__":
    raise SystemExit(main())
