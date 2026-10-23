#!/usr/bin/env python3
"""Exercise native simulations and their original PNG exporters.

Restored games use Linux X11/GLX; Courtyard and Orbit Breaker use headless
software hosts. Existing binaries are used unless --build is requested.

Examples::

    python3 -B scripts/smoke-games.py --game owned-orbit --profile debug
    python3 -B scripts/smoke-games.py --build --game emberline --json

Every selected game has build, launch and capture outcomes in smoke-results.json.
Report schema 2 retains schema 1 fields and adds artifact provenance: build.artifact
and capture.artifact record SHA-256, byte size, filesystem identity and Unix
nanosecond timestamps. Binary freshness is 'build_verified' only after this run's
successful Cargo build (which may reuse an unchanged binary); prebuilt freshness
stays 'unverified'. launch.binary_after/binary_unchanged detect concurrent changes.
Builds select the executable from Cargo's JSON compiler-artifact, matching the
game's manifest and binary target, and require a successful build-finished record.
build.cargo_artifact retains that selection evidence, including Cargo's 'fresh'
cache flag. Configured targets and custom target directories need no path guesses.
Build output is spooled, parsed within size limits, then bounded in the report;
truncated or invalid artifact evidence never verifies an assumed executable.

Commands record argv, cwd, timeout_seconds, started_at_ns/finished_at_ns and an
explicit allowlist of host/render environment values, including unset values.
The rest of the inherited environment is deliberately not serialized. Captures
use a private temporary path and filesystem mtime boundaries around the launch;
only fresh, validated captures replace the legacy <game>.png output. Identical
pixels across runs are valid. These checks establish local artifact provenance,
not source-tree identity or protection against deliberately forged timestamps.
Environment limitations are blocked, never passed. This does not test browser or
device behavior, gameplay completion, visual quality or frame-rate performance.
"""
import argparse
from collections import Counter
from contextlib import ExitStack
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import sys
import tempfile
import time
import zlib

ROOT = Path(__file__).resolve().parent.parent
HEADLESS = {"owned-lab", "owned-orbit"}
MAX_BUILD_OUTPUT = 8 * 1024 * 1024
MAX_BUILD_MESSAGE = 1024 * 1024
MAX_BUILD_MESSAGES = 10000
MAX_BUILD_REPORT_OUTPUT = 16 * 1024
REPORTED_ENVIRONMENT = (
    "DISPLAY", "LIBGL_ALWAYS_SOFTWARE", "MESA_LOADER_DRIVER_OVERRIDE", "GALLIUM_DRIVER", "DRI_PRIME",
    "FERRAVEX_QUALITY", "FERRAVEX_LOOK", "FERRAVEX_MAX_FPS", "FERRAVEX_DPI_SCALE",
    "FERRAVEX_PLAYTEST", "FERRAVEX_AUDIO", "FERRAVEX_AUDIO_DEVICE", "FERRAVEX_AUDIO_CAPTURE",
    "FERRAVEX_SAVE_DIR", "FERRAVEX_NATIVE_SCREENSHOT",
)


def validate_samples(samples):
    """Reject clear-only/tiny-error output without requiring an antialiased palette.

    Mesa may render authored flat colors exactly, where a hardware MSAA resolve
    adds dozens of edge shades. Color count alone is not a portable scene check.
    """
    colors = Counter(samples)
    total = colors.total()
    foreground = 1 - max(colors.values(), default=0) / max(1, total)
    if len(colors) < 4 or foreground < 0.02 or not any(color[3] for color in colors):
        raise ValueError("blank or incomplete GPU capture")
    return {"sampled_colors": len(colors), "foreground_fraction": round(foreground, 4)}


def validate_capture(path):
    """Validate the engine's RGBA8/filter-0 PNG export and return its metrics.

    CRCs, chunk boundaries, header/layout, complete zlib stream and scene
    coverage must all pass. Decompression is bounded by the declared image size.
    This accepts the original flat-color palette on both Mesa and hardware.
    """
    data = Path(path).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("missing or invalid PNG capture")
    offset = 8
    width = height = None
    compressed = bytearray()
    ended = False
    while offset < len(data):
        if offset + 12 > len(data):
            raise ValueError("truncated PNG chunk")
        length = int.from_bytes(data[offset:offset + 4], "big")
        end = offset + length + 12
        if end > len(data):
            raise ValueError("truncated PNG chunk")
        kind = data[offset + 4:offset + 8]
        chunk = data[offset + 8:end - 4]
        checksum = int.from_bytes(data[end - 4:end], "big")
        if zlib.crc32(kind + chunk) != checksum:
            raise ValueError("invalid PNG chunk checksum")
        if width is None and kind != b"IHDR":
            raise ValueError("PNG must begin with IHDR")
        if kind == b"IHDR":
            if width is not None or length != 13:
                raise ValueError("invalid PNG header")
            width, height, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", chunk)
            if min(width, height) < 100:
                raise ValueError("capture is too small")
            if width * height > 32 * 1024 * 1024:
                raise ValueError("capture exceeds smoke image size limit")
            if (depth, color, compression, filtering, interlace) != (8, 6, 0, 0, 0):
                raise ValueError("unexpected original PNG pixel layout")
        elif kind == b"IDAT":
            compressed.extend(chunk)
        elif kind == b"IEND":
            if length or end != len(data):
                raise ValueError("invalid PNG end chunk or trailing bytes")
            ended = True
        offset = end
    if not ended or not compressed:
        raise ValueError("incomplete PNG capture")
    stride = width * 4 + 1
    expected = stride * height
    try:
        stream = zlib.decompressobj()
        raw = stream.decompress(compressed, expected + 1)
    except zlib.error as error:
        raise ValueError(f"invalid PNG compressed data: {error}") from error
    if len(raw) != expected or not stream.eof or stream.unused_data or any(raw[y * stride] for y in range(height)):
        raise ValueError("unexpected original PNG scanline layout")
    coverage = validate_samples(raw[y * stride + 1 + x * 4:y * stride + 5 + x * 4]
                                for y in range(0, height, 7) for x in range(0, width, 7))
    return {"width": width, "height": height, **coverage, "sha256": hashlib.sha256(data).hexdigest()}


def launch_options(game, frames, screenshot):
    """Keep original per-host CLI flags, including Strike's practice mode."""
    if game == "owned-lab":
        return ["--frames", str(frames), "--output", str(screenshot)]
    if game == "owned-orbit":
        return ["--autoplay", "--frames", str(frames), "--screenshot", str(screenshot)]
    if game == "strike":
        return ["--play", "--smoke-frames", str(frames), "--screenshot", str(screenshot)]
    return ["--autoplay", "--smoke-frames", str(frames), "--screenshot", str(screenshot)]


def _text(value):
    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""


def _file_identity(info):
    return {"device": info.st_dev, "inode": info.st_ino, "mode": info.st_mode,
            "size_bytes": info.st_size, "mtime_ns": info.st_mtime_ns, "ctime_ns": info.st_ctime_ns}


def _same_stat_views(path_identity, handle_identity):
    # CPython on Windows reports creation time through stat(path), but change
    # time through fstat(fd). Compare each API's ctime against its own baseline.
    # Cross-API identity, mode, size and modification time must still agree.
    return all(value == handle_identity[key] for key, value in path_identity.items()
               if key != "ctime_ns" or sys.platform != "win32")


def _artifact(path, *, reject_symlink=False):
    """Snapshot a stable regular file with bounded-memory hashing, or explain why not."""
    result = {"path": str(path), "freshness": "unverified"}
    try:
        info = path.lstat() if reject_symlink else path.stat()
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("artifact must be a regular file, not a directory, device or capture symlink")
        identity = _file_identity(info)
        result.update(identity)
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            opened = _file_identity(os.fstat(stream.fileno()))
            if not _same_stat_views(identity, opened):
                raise ValueError("artifact changed before hashing")
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
            after = _file_identity(os.fstat(stream.fileno()))
        current = _file_identity(path.lstat() if reject_symlink else path.stat())
        if after != opened or current != identity:
            raise ValueError("artifact changed while hashing")
        result.update(status="present", sha256=digest.hexdigest())
    except FileNotFoundError as error:
        result.update(status="missing", error=str(error))
    except (OSError, ValueError) as error:
        result.update(status="unavailable", error=str(error))
    result["observed_at_ns"] = time.time_ns()
    return result


def _same_artifact(before, after):
    keys = ("device", "inode", "mode", "size_bytes", "mtime_ns", "ctime_ns", "sha256")
    return (before["status"] == after["status"] == "present"
            and all(before[key] == after[key] for key in keys))


def _capture_boundary(directory, name):
    # Use the capture filesystem's own clock/resolution, rather than comparing
    # its rounded mtime to Python's higher-resolution wall clock.
    marker = directory / name
    marker.touch(exist_ok=False)
    return marker.stat().st_mtime_ns


def _command(command, timeout, root, runner, environ, *, output_limits=None):
    started = time.monotonic()
    result = {"command": list(command), "cwd": str(root), "timeout_seconds": timeout,
              "environment": {key: environ.get(key) for key in REPORTED_ENVIRONMENT},
              "started_at_ns": time.time_ns()}
    with ExitStack() as stack:
        streams = {}
        try:
            # Builds can emit many JSON records. Spool subprocess output rather
            # than buffering an unbounded stream in Python; read only our budget.
            if output_limits is not None:
                streams = {key: stack.enter_context(tempfile.TemporaryFile()) for key in output_limits}
            options = streams if output_limits is not None else {"capture_output": True}
            process = runner(command, cwd=root, text=True, errors="replace", timeout=timeout, env=environ, **options)
            result.update(status="passed" if process.returncode == 0 else "failed", exit_code=process.returncode,
                          stdout=process.stdout, stderr=process.stderr)
        except subprocess.TimeoutExpired as error:
            result.update(status="timeout", error=f"timed out after {timeout}s", stdout=error.stdout, stderr=error.stderr)
        except OSError as error:
            result.update(status="unavailable", error=str(error))
        for key in ("stdout", "stderr"):
            value = result.get(key)
            if key in streams:
                limit = output_limits[key]
                if value is None:
                    size = streams[key].seek(0, os.SEEK_END)
                    # Keep final compiler errors even after verbose diagnostics.
                    streams[key].seek(max(0, size - limit) if key == "stderr" else 0)
                    value = streams[key].read(limit)
                else:
                    size = len(value)
                # Injected subprocess-compatible runners may return output
                # directly instead of writing the supplied streams.
                result[key + "_truncated"] = size > limit
                value = value[-limit:] if key == "stderr" else value[:limit]
            if key in result or key in streams:
                result[key] = _text(value)
        if result["status"] == "failed":
            result["error"] = f"exit {result['exit_code']}: {result['stderr'][-1500:]}"
    result["finished_at_ns"] = time.time_ns()
    result["wall_seconds"] = round(time.monotonic() - started, 3)
    return result


def _cargo_executable(output, game, root):
    """Select one real binary artifact from a complete, successful Cargo stream.

    Package IDs are opaque; the catalog's manifest path identifies the package.
    Libraries, build scripts and other dependency targets are not candidates.
    No profile/target-directory guess, timestamp or filename search substitutes
    for the selected compiler-artifact's executable field.
    """
    if len(output) > MAX_BUILD_OUTPUT:
        raise ValueError(f"Cargo JSON exceeds {MAX_BUILD_OUTPUT} characters")
    expected_manifest = (root / "examples" / game / "Cargo.toml").resolve()
    selected = None
    finished = False
    # StringIO avoids allocating a list containing the entire output again.
    for number, line in enumerate(io.StringIO(output), 1):
        if number > MAX_BUILD_MESSAGES or len(line) > MAX_BUILD_MESSAGE:
            raise ValueError("Cargo JSON exceeds the message count/size limit")
        if not line.strip():
            continue
        if finished:
            raise ValueError("unexpected Cargo output after build-finished")
        try:
            message = json.loads(line)
        except (ValueError, RecursionError) as error:
            raise ValueError(f"malformed Cargo JSON at line {number}") from error
        if not isinstance(message, dict) or not isinstance(message.get("reason"), str):
            raise ValueError(f"invalid Cargo message at line {number}: expected an object with reason")
        reason = message["reason"]
        if reason == "build-finished":
            if message.get("success") is not True:
                raise ValueError("Cargo build-finished did not report success=true")
            finished = True
        elif reason == "compiler-artifact":
            target = message.get("target")
            if (not isinstance(target, dict) or not isinstance(target.get("name"), str)
                    or not isinstance(target.get("kind"), list)):
                raise ValueError(f"malformed Cargo compiler-artifact target at line {number}")
            if target["name"] != game or target["kind"] != ["bin"]:
                continue
            if target.get("crate_types") != ["bin"]:
                raise ValueError("Cargo binary artifact must have crate_types=['bin']")
            manifest = message.get("manifest_path")
            if not isinstance(manifest, str) or not manifest or "\x00" in manifest or not Path(manifest).is_absolute():
                raise ValueError("Cargo binary artifact has an invalid manifest_path")
            if Path(manifest).resolve() != expected_manifest:
                raise ValueError(f"Cargo binary artifact is unrelated: expected manifest {expected_manifest}")
            profile = message.get("profile")
            if not isinstance(profile, dict) or profile.get("test") is not False:
                raise ValueError("Cargo binary artifact must be a non-test executable (profile.test=false)")
            executable = message.get("executable")
            if (not isinstance(executable, str) or not executable or "\x00" in executable
                    or not Path(executable).is_absolute()):
                raise ValueError("Cargo binary artifact lacks an absolute executable path")
            filenames = message.get("filenames")
            if (not isinstance(filenames, list) or executable not in filenames
                    or any(not isinstance(name, str) or "\x00" in name or not Path(name).is_absolute()
                           for name in filenames)):
                raise ValueError("Cargo binary artifact has invalid filenames or omits its executable")
            package = message.get("package_id")
            if not isinstance(package, str) or not package or type(message.get("fresh")) is not bool:
                raise ValueError("Cargo binary artifact has invalid package_id/fresh evidence")
            if selected is not None:
                raise ValueError("ambiguous Cargo binary artifacts; configure a single build target for smoke")
            selected = {"package_id": package, "manifest_path": manifest,
                        "target": {"name": game, "kind": ["bin"]}, "executable": executable,
                        "fresh": message["fresh"]}
    if not finished:
        raise ValueError("Cargo JSON is missing a successful build-finished record")
    if selected is None:
        raise ValueError(f"Cargo JSON has no compiler-artifact executable for {game} at {expected_manifest}")
    return Path(selected["executable"]), selected


def _environment_limit(game, environ, platform):
    if game in HEADLESS:
        return None
    if not platform.startswith("linux"):
        return "This windowed smoke path requires Linux X11/GLX; use a supported Linux display. Browser/device hosts are not exercised."
    if not environ.get("DISPLAY", "").strip():
        return "DISPLAY is unset. Use a working X11/GLX display (or Xvfb with GLX); owned-lab and owned-orbit can run headlessly."
    return None


def run_game(game, binaries, args, root=ROOT, runner=None, environ=None, platform=None):
    """Record independent build/launch/capture outcomes for one game.

    Runner/environment injection allows display-free failure tests. An existing
    binary is reported as 'prebuilt', never as a verified current-source build.
    With --build, binaries is only a diagnostic fallback; Cargo JSON selects the
    launched executable. A successful exit alone never verifies a guessed path.
    Capture freshness requires a unique initially absent launch path, an mtime
    within capture.freshness_window, and stable bytes/identity through validation.
    The reported screenshot remains the legacy output path; requested_path is
    the exact temporary path passed to the host, removed after this call.
    """
    runner = runner or subprocess.run
    environ = dict(os.environ if environ is None else environ)
    platform = platform or sys.platform
    root = Path(root).resolve()
    binaries = Path(binaries)
    if not binaries.is_absolute():
        binaries = root / binaries
    started = time.monotonic()
    result = {"game": game, "frames": args.frames, "passed": False, "status": "failed",
              "host": "headless-software" if game in HEADLESS else "linux-x11-glx",
              "build": {"status": "not_run"}, "launch": {"status": "skipped"}, "capture": {"status": "skipped"}}
    try:
        output = args.output.resolve()
        screenshot = output / f"{game}.png"
        output.mkdir(parents=True, exist_ok=True)
        # A previous successful run must never mask this run's failure.
        # Resolve only the parent, so an old capture symlink cannot delete its target.
        screenshot.unlink(missing_ok=True)
        executable = binaries / game
        if not executable.is_file() and executable.with_suffix(".exe").is_file():
            executable = executable.with_suffix(".exe")
        if args.build:
            command = ["cargo", "build", "--offline", "--locked", "-j", "2", "--message-format=json", "--profile",
                       "dev" if args.profile == "debug" else args.profile, "-p", game, "--bin", game]
            build = _command(command, args.build_timeout, root, runner, environ,
                             output_limits={"stdout": MAX_BUILD_OUTPUT, "stderr": MAX_BUILD_REPORT_OUTPUT})
            result["build"] = build
            if build["status"] == "passed":
                try:
                    if build.get("stdout_truncated"):
                        raise ValueError(f"Cargo JSON exceeds the {MAX_BUILD_OUTPUT}-byte output limit")
                    executable, evidence = _cargo_executable(build["stdout"], game, root)
                    build["cargo_artifact"] = evidence
                except (OSError, ValueError, RuntimeError) as error:
                    build.update(status="failed", error=f"cannot verify Cargo executable: {error}; "
                                 "check Cargo target configuration and JSON output; no binary was launched")
            if len(build.get("stdout", "")) > MAX_BUILD_REPORT_OUTPUT:
                build["stdout"] = build["stdout"][:MAX_BUILD_REPORT_OUTPUT]
                build["stdout_truncated"] = True
        result["build"]["executable"] = str(executable)
        artifact = _artifact(executable)
        result["build"]["artifact"] = artifact
        if args.build and result["build"]["status"] != "passed":
            result["error"] = "build: " + result["build"]["error"]
            return result
        if artifact["status"] == "missing":
            error = (f"Cargo-reported executable is missing: {executable}; check build output and concurrent cleanup"
                     if args.build else "native binary is missing; build it first or pass --build")
            result["build"].update(status="missing", error=error)
            result["error"] = result["build"]["error"]
            return result
        if artifact["status"] != "present":
            result["build"].update(status="unavailable", error=artifact["error"])
            result["error"] = "binary provenance: " + artifact["error"]
            return result
        if args.build:
            artifact["freshness"] = "build_verified"
        if not args.build:
            result["build"].update(status="prebuilt", note="Build not run; existing artifact freshness is unverified.")
        limitation = _environment_limit(game, environ, platform)
        if limitation:
            result["launch"] = {"status": "blocked", "error": limitation}
            result.update(status="blocked", error=limitation, environment_limitation=limitation)
            return result
        with tempfile.TemporaryDirectory(prefix=f".{game}-smoke-", dir=output) as directory:
            directory = Path(directory)
            requested = directory / screenshot.name
            capture = result["capture"]
            capture.update(requested_path=str(requested), freshness_window={
                "method": "isolated-path-and-filesystem-mtime",
                "not_before_mtime_ns": _capture_boundary(directory, "launch-start"),
            })
            command = [str(executable), *launch_options(game, args.frames, requested)]
            result["launch"] = _command(command, args.timeout, root, runner, environ)
            capture["freshness_window"]["not_after_mtime_ns"] = _capture_boundary(directory, "launch-end")
            capture["artifact"] = _artifact(requested, reject_symlink=True)
            captured = capture["artifact"]
            if captured["status"] == "present":
                lower = capture["freshness_window"]["not_before_mtime_ns"]
                upper = capture["freshness_window"]["not_after_mtime_ns"]
                captured["freshness"] = "fresh" if lower <= captured["mtime_ns"] <= upper else "stale"
            after = _artifact(executable)
            result["launch"].update(binary_after=after, binary_unchanged=_same_artifact(artifact, after))
            # Retain original top-level fields for report consumers.
            for key in ("exit_code", "stdout", "stderr"):
                if key in result["launch"]:
                    result[key] = result["launch"][key]
            if result["launch"]["status"] != "passed":
                result["error"] = "launch: " + result["launch"]["error"]
                return result
            if not result["launch"]["binary_unchanged"]:
                result["error"] = "binary provenance: executable changed or became unavailable during launch"
                return result
            capture["status"] = "failed"
            try:
                if captured["status"] != "present":
                    raise ValueError(captured["error"])
                if captured["freshness"] != "fresh":
                    raise ValueError("stale capture: modification time is outside this launch's filesystem window")
                metrics = validate_capture(requested)
                identity = _file_identity(requested.lstat())
                if (metrics["sha256"] != captured["sha256"]
                        or identity != {key: captured[key] for key in identity}):
                    captured["freshness"] = "changed"
                    raise ValueError("capture changed during validation")
                requested.replace(screenshot)
                published = _artifact(screenshot, reject_symlink=True)
                # Rename may change ctime; all other identity/content evidence
                # must still match the capture we actually checked.
                keys = ("device", "inode", "mode", "size_bytes", "mtime_ns", "sha256")
                if (published["status"] != "present"
                        or any(published[key] != captured[key] for key in keys)):
                    captured["freshness"] = "changed"
                    raise ValueError("capture changed during publication")
                published["freshness"] = "fresh"
                capture["artifact"] = published
            except (OSError, ValueError) as error:
                capture["error"] = str(error)
                result["error"] = "capture: " + str(error)
                return result
        capture_path = str(screenshot.relative_to(root)) if screenshot.is_relative_to(root) else str(screenshot)
        result["capture"].update(status="passed", screenshot=capture_path, **metrics)
        result.update(passed=True, status="passed", screenshot=capture_path, **metrics)
        return result
    except OSError as error:
        result["error"] = str(error)
        return result
    finally:
        for stage in ("launch", "capture"):
            if result[stage]["status"] == "skipped":
                result[stage]["reason"] = result.get("error", "previous stage did not complete")
        result["wall_seconds"] = round(time.monotonic() - started, 3)


def run_smoke(games, args, root=ROOT, runner=None, environ=None, platform=None, progress=None):
    """Cover every selected game in a report, even if prerequisites fail."""
    runner = runner or subprocess.run
    environ = dict(os.environ if environ is None else environ)
    platform = platform or sys.platform
    root = Path(root).resolve()
    metadata = _command(["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline", "--locked"],
                        args.timeout, root, runner, environ)
    binaries = None
    if metadata["status"] == "passed":
        try:
            target = json.loads(metadata["stdout"])["target_directory"]
            if not isinstance(target, str) or not target:
                raise ValueError("target_directory must be a nonempty path")
            binaries = Path(target) / args.profile
            if not binaries.is_absolute():
                binaries = root / binaries
        except (ValueError, KeyError, TypeError) as error:
            metadata.update(status="failed", error=f"invalid cargo metadata: {error}")
    metadata.pop("stdout", None)
    results = []
    for game in games:
        if binaries is None:
            reason = "cannot locate native artifacts: " + metadata["error"]
            result = {"game": game, "frames": args.frames, "passed": False, "status": "blocked",
                      "build": {"status": "unavailable", "error": reason},
                      "launch": {"status": "skipped", "reason": reason},
                      "capture": {"status": "skipped", "reason": reason}, "error": reason, "wall_seconds": 0.0}
        else:
            result = run_game(game, binaries, args, root, runner, environ, platform)
        results.append(result)
        if progress:
            progress(result)
    summary = {status: sum(result["status"] == status for result in results) for status in ("passed", "failed", "blocked")}
    summary["games"] = len(results)
    summary["stages"] = {stage: dict(sorted(Counter(result[stage]["status"] for result in results).items())) for stage in ("build", "launch", "capture")}
    return {"schema_version": 2, "profile": args.profile, "passed": bool(results) and all(result["passed"] for result in results),
            "note": "Local simulation, rendering and PNG export checks, not performance or gameplay-completion measurements.",
            "environment": {"platform": platform, "display_configured": bool(environ.get("DISPLAY", "").strip()),
                            "note": "DISPLAY presence does not verify X11/GLX access. Headless software passes do not validate GPU, browser or devices."},
            "metadata": metadata, "summary": summary, "results": results}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release", "small"), default="release")
    parser.add_argument("--frames", type=int, default=720)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--game", action="append", help="only this game; repeat for a subset")
    parser.add_argument("--output", type=Path, default=ROOT / "target" / "game-smoke")
    parser.add_argument("--build", action="store_true", help="build each selected binary offline, with at most two jobs")
    parser.add_argument("--build-timeout", type=int, default=600, help="per-game build timeout in seconds")
    parser.add_argument("--json", action="store_true", help="write only the machine-readable report to stdout")
    args = parser.parse_args(argv)
    if args.frames < 1 or args.timeout < 1 or args.build_timeout < 1:
        parser.error("frames and timeouts must be positive")
    try:
        catalog = json.loads((ROOT / "examples" / "catalog.json").read_text(encoding="utf-8"))
        if not isinstance(catalog, list) or not catalog:
            raise ValueError("catalog must be a nonempty array")
        games = [game["id"] for game in catalog]
        if any(not isinstance(game, str) or not re.fullmatch(r"[a-z][a-z0-9]*(?:-[a-z0-9]+)*", game) for game in games):
            raise ValueError("invalid game id")
        if len(set(games)) != len(games):
            raise ValueError("duplicate game id")
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(f"invalid catalog: {error}; run scripts/audit-catalog.py")
    if args.game:
        unknown = set(args.game) - set(games)
        if unknown:
            parser.error("unknown games: " + ", ".join(sorted(unknown)))
        games = [game for game in games if game in args.game]

    def progress(result):
        stages = ", ".join(f"{stage}={result[stage]['status']}" for stage in ("build", "launch", "capture"))
        print(f"{result['status'].upper()} {result['game']}: {stages}" + (f"; {result['error']}" if "error" in result else ""), flush=True)

    report = run_smoke(games, args, root=ROOT, progress=None if args.json else progress)
    encoded = json.dumps(report, indent=2) + "\n"
    try:
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "smoke-results.json").write_text(encoded, encoding="utf-8")
    except OSError as error:
        print(f"Cannot write smoke report: {error}", file=sys.stderr)
        if args.json:
            print(encoded, end="")
        return 1
    if args.json:
        print(encoded, end="")
    else:
        print(json.dumps(report["summary"], sort_keys=True))
        print(report["environment"]["note"])
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
