#!/usr/bin/env python3
"""Inspect Ferravex prerequisites without installing, building, or changing settings.

Usage::

    python3 -B scripts/doctor.py
    python3 -B scripts/doctor.py --profile web --json
    python3 -B scripts/doctor.py --profile core

``all`` (the default) checks the full development environment. Every profile
requires the Rust/Python toolchain. ``core`` adds the native linker; ``web`` adds
the active compiler's Wasm standard library and Node 22+; ``native`` adds native
build and display prerequisites. Unselected capabilities and development conveniences
are still reported, but are optional. Linux native readiness includes a GLX
probe using glxinfo; this diagnostic tool is required for that verification.

Exit codes: 0 = all required checks passed, 1 = a required check failed or could
not be verified, 2 = invalid CLI arguments, 130 = interrupted. JSON has a versioned
schema and is the only stdout output with --json. Suggested commands are never
executed. No check compiles code, opens a game, downloads tools, or writes files.

JSON schema 2 retains the schema 1 fields and adds ``executable``, ``version``
and ``probe`` to every check. Executables are absolute invocation paths, keeping
symlink names intact for dispatchers such as rustup. Versions are numeric when
a known format is parsed, otherwise the first version-output line, or null if
not queried/recognized. Version/detail/stream summaries have a 500-character
limit and explicitly mark truncation. A null probe means the check was skipped
or only inspected presence. Probe objects contain ``attempted``, ``command``,
``failure``, ``returncode``, ``timeout_seconds``, ``output_limit_bytes``, ``stdout``
and ``stderr``. Failure is null for successful queries or one of ``not_found``
(attempted=false), ``execution_error``, ``timeout``, ``output_limit``, and
``nonzero_exit``. Returncode is null if no natural exit was observed.
``missing`` means lookup/capability absence; a found tool whose query fails is
``error``. Presence alone is ``unverified`` for tools with no safe query.
Required and optional checks use identical evidence and failure semantics.

Each query captures at most 64 KiB across both streams. Timeouts/output overflow
stop the query; cleanup may take up to one extra second. POSIX queries run in a
separate process group so their helpers can also be stopped. On other platforms
only the direct process is stopped. OS process creation itself is not timed.
"""

import argparse
import json
import math
import os
from pathlib import Path
import queue
import re
import shutil
import signal
import subprocess
import sys
import threading
import time


ROOT = Path(__file__).resolve().parent.parent
PROFILES = ("all", "core", "web", "native")
WASM_TARGET = "wasm32-unknown-unknown"
PROBE_OUTPUT_LIMIT = 64 * 1024
SUMMARY_LIMIT = 500
RUST_HELP = "Install Rust with Cargo from https://rustup.rs/; then run rustup update stable."
PYTHON_HELP = "Install Python 3.11+ and ensure python3 --version selects it."
NODE_HELP = "Install Node.js 22+ (CI uses Node 24); then run node --version."
LINUX_LIB_HELP = (
    "Install the X11/OpenGL development libraries for your distribution. "
    "Debian/Ubuntu: sudo apt-get install libx11-dev libgl1-mesa-dev"
)
DISPLAY_HELP = (
    "Run from an X11/XWayland desktop terminal, or install xvfb and mesa-utils "
    "and run: LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a python3 -B scripts/doctor.py --profile native"
)


def _summary(text):
    """Bound diagnostics and keep terminal control characters out of human output."""
    text = " ".join("".join(c if c.isprintable() else " " for c in text).split())
    suffix = "... [truncated]"
    return text if len(text) <= SUMMARY_LIMIT else text[:SUMMARY_LIMIT - len(suffix)] + suffix


def _decode(output):
    return output.decode("utf-8", errors="replace") if isinstance(output, bytes) else output or ""


class _OutputLimitExceeded(Exception):
    def __init__(self, stdout, stderr):
        super().__init__("Query output limit exceeded.")
        self.stdout, self.stderr = stdout, stderr


def _run_bounded(command, *, cwd, env, stdin, timeout, shell):
    """Capture pipes in bounded chunks without files or unbounded communicate()."""
    deadline = time.monotonic() + timeout
    process = subprocess.Popen(
        command, cwd=cwd, env=env, stdin=stdin, shell=shell,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0,
        start_new_session=os.name == "posix",
    )
    chunks = queue.Queue(maxsize=4)
    stopped = threading.Event()
    output = [bytearray(), bytearray()]

    def send(item):
        while not stopped.is_set():
            try:
                chunks.put(item, timeout=0.05)
                return
            except queue.Full:
                pass

    def read_pipe(index, stream):
        # Daemon readers keep inherited pipes from blocking return on platforms
        # without process-group cleanup. Each queue entry is at most 4096 bytes.
        with stream:
            try:
                while not stopped.is_set():
                    data = stream.read(4096)
                    send((index, data))
                    if not data:
                        return
            except OSError as error:
                send((index, error))

    try:
        for index, stream in enumerate((process.stdout, process.stderr)):
            threading.Thread(target=read_pipe, args=(index, stream), daemon=True).start()
        closed = 0
        while closed < 2:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise subprocess.TimeoutExpired(command, timeout)
            try:
                index, data = chunks.get(timeout=min(remaining, threading.TIMEOUT_MAX))
            except queue.Empty:
                continue
            if isinstance(data, OSError):
                raise data
            if not data:
                closed += 1
                continue
            available = PROBE_OUTPUT_LIMIT - sum(map(len, output))
            output[index].extend(data[:available])
            if len(data) > available:
                raise _OutputLimitExceeded(bytes(output[0]), bytes(output[1]))
        while True:
            remaining = deadline - time.monotonic()
            try:
                process.wait(timeout=max(0, min(remaining, 1.0)))
                break
            except subprocess.TimeoutExpired:
                if time.monotonic() >= deadline:
                    raise
        return subprocess.CompletedProcess(
            command, process.returncode, _decode(bytes(output[0])), _decode(bytes(output[1])))
    except subprocess.TimeoutExpired:
        raise subprocess.TimeoutExpired(
            command, timeout, output=bytes(output[0]), stderr=bytes(output[1])) from None
    finally:
        stopped.set()
        # Kill the private group even if its leader exited but helpers kept the
        # pipes open. Never wait on inherited pipes or join a blocked reader.
        try:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            elif process.poll() is None:
                process.kill()
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=1.0)
        except subprocess.TimeoutExpired:
            pass


def _executable(name):
    """Resolve PATH once, without dereferencing a shim's executable name."""
    path = shutil.which(name)
    return os.path.abspath(path) if path else None


def _probe(command, timeout, *, executable=None):
    """Return status, raw bounded output/detail, and JSON-compatible evidence."""
    executable = executable or _executable(command[0])
    probe = {
        "attempted": False, "command": [], "failure": None, "returncode": None,
        "timeout_seconds": timeout, "output_limit_bytes": PROBE_OUTPUT_LIMIT,
        "stdout": "", "stderr": "",
    }
    evidence = {"executable": executable, "probe": probe}
    if not executable:
        probe["failure"] = "not_found"
        return "missing", f"{command[0]} is not on PATH.", evidence
    probe["command"] = [executable, *command[1:]]
    probe["attempted"] = True
    env = os.environ.copy()
    env.update(
        RUSTUP_AUTO_INSTALL="0", RUSTUP_NO_UPDATE_CHECK="1",
        CARGO_NET_OFFLINE="true", PYTHONDONTWRITEBYTECODE="1", LC_ALL="C",
    )
    try:
        result = _run_bounded(
            probe["command"], cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
            timeout=timeout, shell=False,
        )
    except subprocess.TimeoutExpired as error:
        probe["failure"] = "timeout"
        stdout, stderr = _decode(error.output), _decode(error.stderr)
        detail = f"Query timed out after {timeout:g} seconds."
    except _OutputLimitExceeded as error:
        probe["failure"] = "output_limit"
        stdout, stderr = _decode(error.stdout), _decode(error.stderr)
        detail = f"Query exceeded {PROBE_OUTPUT_LIMIT} bytes of combined output."
    except OSError as error:
        # ENOENT after a successful lookup can mean a missing script interpreter
        # or loader. It is an unusable resolved executable, not a PATH absence.
        probe["failure"] = "execution_error"
        return "error", f"Cannot execute/read resolved query: {_summary(str(error))}", evidence
    else:
        stdout, stderr = result.stdout, result.stderr
        probe["returncode"] = result.returncode
        if result.returncode:
            probe["failure"] = "nonzero_exit"
        detail = f"Query exited {result.returncode}."
    probe["stdout"], probe["stderr"] = _summary(stdout), _summary(stderr)
    if probe["failure"]:
        for name in ("stderr", "stdout"):
            if probe[name]:
                detail += f" {name}: {probe[name]}"
        return "error", _summary(detail), evidence
    return "ok", stdout.strip() or stderr.strip(), evidence


def _check(check_id, name, required, status, detail, action="", command=(), *,
           executable=None, version=None, probe=None):
    return {
        "id": check_id, "name": name, "required": required, "status": status,
        "detail": _summary(detail), "action": action if status != "ok" else "",
        "command": list(command),
        "executable": executable, "version": version, "probe": probe,
    }


def _tool(check_id, name, executable, required, action, timeout, *,
          arguments=("--version",), pattern=None, minimum=None):
    command = [executable, *arguments]
    status, output, evidence = _probe(command, timeout)
    version = None
    if status == "ok" and not output:
        status, output = "error", "Query returned no version information."
    if status == "ok" and pattern:
        match = re.search(pattern, output)
        if not match:
            status, output = "error", f"Unrecognized version output: {_summary(output) or '(empty)'}"
        else:
            version = ".".join(match.groups())
            # Compare digit strings without converting untrusted, arbitrarily
            # long version components to Python integers.
            if minimum and _version_key(match.groups()) < _version_key(map(str, minimum)):
                status, output = "too_old", f"{_summary(output)}; requires {'.'.join(map(str, minimum))} or newer."
    elif status == "ok":
        version = _summary(output.splitlines()[0])
    return _check(check_id, name, required, status, output, action, command,
                  version=_summary(version) if version else None, **evidence)


def _version_key(parts):
    return tuple((len(part.lstrip("0") or "0"), part.lstrip("0") or "0") for part in parts)


def _rust_minimum():
    # Only read the workspace.package section. Avoid requiring tomllib/Python
    # 3.11 merely to report that this prerequisite is missing.
    manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"(?ms)^\[workspace\.package\]\s*\n(.*?)(?=^\[|\Z)", manifest)
    match = re.search(r'^rust-version\s*=\s*["\'](\d+)\.(\d+)(?:\.(\d+))?["\']',
                      section.group(1) if section else "", re.M)
    if not match:
        raise ValueError("Cargo.toml has no readable workspace.package rust-version.")
    return tuple(int(n or 0) for n in match.groups())


def _wasm_check(required, rust, timeout):
    action = "For the active Rust toolchain, run: rustup target add wasm32-unknown-unknown"
    command = ["rustc", "--print", "target-libdir", "--target", WASM_TARGET]
    if rust["status"] != "ok":
        return _check("rust.wasm", "Wasm standard library", required, "unverified",
                      "Cannot verify the target until the Rust compiler check passes.", RUST_HELP, command,
                      executable=rust["executable"])
    status, output, evidence = _probe(command, timeout, executable=rust["executable"])
    if status == "ok":
        try:
            if not output or any(c in output for c in "\r\n\0"):
                raise ValueError("rustc did not return a single target library directory.")
            directory = Path(output)
            if not directory.is_absolute():
                directory = ROOT / directory
            # rustc --print target-libdir succeeds even when the target is absent.
            # A rustup target list for a different compiler is not sufficient.
            found_names = set()
            if directory.is_dir():
                with os.scandir(directory) as entries:
                    for entry in entries:
                        for name in ("core", "std"):
                            if (entry.name.startswith(f"lib{name}-") and entry.name.endswith(".rlib")
                                    and entry.is_file() and entry.stat().st_size > 0):
                                found_names.add(name)
                        if len(found_names) == 2:
                            break
            found = len(found_names) == 2
            status = "ok" if found else "missing"
            output = f"{WASM_TARGET}: {directory}" if found else f"No complete Wasm core/std libraries in {directory}."
        except (OSError, ValueError) as error:
            status, output = "error", f"Cannot inspect Wasm libraries: {_summary(str(error))}"
    return _check("rust.wasm", "Wasm standard library", required, status, output, action, command, **evidence)


def _library_check(name, filename, compiler, required, timeout, action=LINUX_LIB_HELP):
    command = ["cc", f"-print-file-name={filename}"]
    evidence = {"executable": compiler}
    if not compiler:
        status, detail = "unverified", "Cannot inspect linker libraries without a C compiler driver."
    else:
        status, detail, evidence = _probe(command, timeout, executable=compiler)
        if status == "ok":
            try:
                if not detail or any(c in detail for c in "\r\n\0"):
                    raise ValueError("C compiler did not return a single library path.")
                path = Path(detail)
                if not path.is_absolute():
                    path = ROOT / path
                found = detail != filename and path.is_file() and path.stat().st_size > 0
                status = "ok" if found else "missing"
                detail = str(path) if found else f"{compiler} cannot locate {filename}."
            except (OSError, ValueError) as error:
                status, detail = "error", f"Cannot inspect {filename}: {_summary(str(error))}"
    return _check(f"native.{name.lower()}", name, required, status, detail, action, command, **evidence)


def _linux_checks(required, compiler, timeout):
    checks = [
        _library_check("X11", "libX11.so", compiler, required, timeout),
        _library_check("OpenGL", "libGL.so", compiler, required, timeout),
        _library_check("ALSA", "libasound.so.2", compiler, False, timeout,
                       "Native audio is optional. Install your distribution's ALSA runtime; Debian/Ubuntu: sudo apt-get install libasound2"),
    ]
    display = os.environ.get("DISPLAY", "").strip()
    detail = f"DISPLAY={_summary(display)}; GLX connectivity is checked separately." if display else "DISPLAY is unset; windowed Linux demos need an X11 display."
    if not display and os.environ.get("WAYLAND_DISPLAY"):
        detail += " Wayland alone is insufficient; enable XWayland."
    checks.append(_check("native.display", "X11 display", required,
                         "ok" if display else "missing", detail, DISPLAY_HELP))
    command = ["glxinfo", "-B"]
    evidence = {"executable": _executable("glxinfo")}
    version_text = None
    if not display:
        status, detail = "unverified", "GLX query skipped because DISPLAY is unset."
    else:
        status, detail, evidence = _probe(command, timeout, executable=evidence["executable"])
        if status == "ok":
            # GLES, compatibility-only, or maximum supported version strings do
            # not demonstrate that a desktop core context can actually be used.
            version = re.search(r"OpenGL core profile version string:\s*(\d+)\.(\d+)", detail)
            if not version:
                status, detail = "unverified", "glxinfo did not report a desktop OpenGL core profile."
            elif _version_key(version.groups()) < _version_key(("3", "3")):
                status, detail = "too_old", f"OpenGL core {version[1]}.{version[2]}; the renderer needs 3.3+."
            else:
                renderer = re.search(r"OpenGL renderer string:\s*([^\n]+)", detail)
                detail = f"OpenGL core {version[1]}.{version[2]}" + (f"; {_summary(renderer[1])}" if renderer else "")
            if version:
                version_text = _summary(".".join(version.groups()))
    checks.append(_check("native.glx", "GLX / OpenGL core", required, status, detail,
                         "Install glxinfo (Debian/Ubuntu: sudo apt-get install mesa-utils) and run glxinfo -B on a working X11 display. " + DISPLAY_HELP, command,
                         version=version_text, **evidence))
    xvfb = _executable("xvfb-run")
    checks.append(_check("tool.xvfb", "Headless X11 runner", False,
                         "unverified" if xvfb else "missing",
                         "xvfb-run is on PATH; starting an X server is outside this read-only check."
                         if xvfb else "xvfb-run is not on PATH.",
                         "Run the suggested native doctor command under xvfb-run to verify it. " + DISPLAY_HELP
                         if xvfb else "Install xvfb for headless rendering (Debian/Ubuntu: sudo apt-get install xvfb).",
                         executable=xvfb))
    return checks


def _native_checks(required, linker_required, timeout):
    checks = []
    if sys.platform.startswith("linux") or sys.platform == "darwin":
        # These Rust hosts default to `cc`. An unrelated clang/gcc executable
        # alone does not prove Cargo has a usable configured linker.
        linker = _tool("native.linker", "Native compiler/linker driver", "cc",
                       linker_required, "Install the platform C toolchain (Debian/Ubuntu: sudo apt-get install build-essential; macOS: xcode-select --install).",
                       timeout)
        checks.append(linker)
        compiler = linker["executable"] if linker["status"] == "ok" else None
        if sys.platform.startswith("linux"):
            checks.extend(_linux_checks(required, compiler, timeout))
        else:
            command = ["xcrun", "--no-cache", "--sdk", "macosx", "--show-sdk-path"]
            status, detail, evidence = _probe(command, timeout)
            if status == "ok":
                try:
                    sdk = Path(detail)
                    if not sdk.is_absolute() or any(c in detail for c in "\r\n\0"):
                        raise ValueError("xcrun did not return an absolute SDK directory.")
                    missing = [name for name in ("AppKit", "OpenGL")
                               if not (sdk / "System/Library/Frameworks" / f"{name}.framework").is_dir()]
                    if missing:
                        status, detail = "missing", "SDK frameworks missing: " + ", ".join(missing)
                except (OSError, ValueError) as error:
                    status, detail = "error", f"Cannot inspect macOS SDK: {_summary(str(error))}"
            checks.append(_check("native.sdk", "macOS SDK / AppKit / OpenGL", required, status,
                                 detail, "Install/select the macOS developer tools; inspect xcode-select -p and xcrun --sdk macosx --show-sdk-path.", command, **evidence))
    elif sys.platform == "win32":
        # MSVC link.exe and GNU gcc use incompatible flags; do not execute an
        # arbitrary `link` found on PATH (it can be a filesystem utility).
        linker = _executable("link.exe") or _executable("gcc")
        checks.append(_check("native.linker", "Windows native linker", linker_required,
                             "unverified" if linker else "missing",
                             "Linker is on PATH; its identity and compatibility with Rust have not been queried."
                             if linker else "Neither link.exe nor gcc is on PATH.",
                             "Use the MSVC Developer Command Prompt/Build Tools or the MinGW toolchain matching rustc -vV.",
                             executable=linker))
    else:
        checks.append(_check("native.host", "Native window host", required, "unsupported",
                             f"No native window host for {sys.platform}; portable simulation and Wasm may still work.",
                             "Use --profile web for browser development, or a Linux, macOS, or Windows host."))
    if sys.platform in ("darwin", "win32"):
        checks.append(_check("native.graphics", "Desktop graphics runtime", False, "unverified",
                             "Display access and driver context creation are not probed on this platform.",
                             "After building, run cargo run -p emberline --offline --locked; Windows needs OpenGL 3.3, macOS uses OpenGL 4.1."))
    return checks


def collect_report(profile="all", timeout=5.0):
    """Return a JSON-compatible report without writing files or modifying settings.

    ``collect_report("web")["ok"]`` tests required browser-development
    prerequisites. Every check has a stable id, required flag, status, detail,
    suggested action and the query command (if one ran or was planned). A check
    is satisfied only when its status is ``ok``. The aggregate never treats an
    unavailable required verification as a pass. Schema 2 adds executable,
    version and probe evidence as documented at module level. The ``runtime``
    object identifies this interpreter; the ``python`` check still verifies the
    documented ``python3`` command against the Python 3.11 minimum.
    """
    if profile not in PROFILES:
        raise ValueError(f"Unknown profile: {profile}")
    if not math.isfinite(timeout) or timeout <= 0:
        raise ValueError("timeout must be positive and finite")
    web = profile in ("all", "web")
    native = profile in ("all", "native")
    checks = []
    try:
        minimum = _rust_minimum()
    except (OSError, ValueError) as error:
        minimum = None
        checks.append(_check("workspace.manifest", "Workspace Rust requirement", True,
                             "error", _summary(str(error)), "Restore/read Cargo.toml at the repository root."))
    rust = _tool("rust.compiler", "Rust compiler", "rustc", True, RUST_HELP, timeout,
                 pattern=r"(?m)^rustc (\d+)\.(\d+)\.(\d+)", minimum=minimum)
    checks.append(rust)
    checks.append(_tool("rust.cargo", "Cargo", "cargo", True, RUST_HELP, timeout,
                        pattern=r"(?m)^cargo (\d+)\.(\d+)\.(\d+)", minimum=minimum))
    checks.append(_tool("python", "Python scripts", "python3", True, PYTHON_HELP, timeout,
                        pattern=r"^Python (\d+)\.(\d+)\.(\d+)", minimum=(3, 11, 0)))
    checks.append(_wasm_check(web, rust, timeout))
    checks.append(_tool("node", "Node browser tests", "node", web, NODE_HELP, timeout,
                        pattern=r"^v(\d+)\.(\d+)\.(\d+)", minimum=(22, 0, 0)))
    checks.extend(_native_checks(native, profile != "web", timeout))
    for check_id, name, executable, action, arguments, pattern in (
        ("tool.rustup", "Rust toolchain manager", "rustup", RUST_HELP, ("--version",),
         r"(?m)^rustup (\d+)\.(\d+)\.(\d+)"),
        ("tool.git", "Git", "git", "Install Git to work with repository history; then run git --version.", ("--version",),
         r"(?m)^git version (\d+)\.(\d+)\.(\d+)"),
        ("tool.rustfmt", "Rust formatter", "rustfmt", "rustup component add rustfmt", ("--version",),
         r"(?m)^rustfmt (\d+)\.(\d+)\.(\d+)"),
        ("tool.clippy", "Rust linter", "cargo", "rustup component add clippy", ("clippy", "--version"),
         r"(?m)^clippy (\d+)\.(\d+)\.(\d+)"),
    ):
        checks.append(_tool(check_id, name, executable, False, action, timeout,
                            arguments=arguments, pattern=pattern))
    required_failures = sum(c["required"] and c["status"] != "ok" for c in checks)
    optional_issues = sum(not c["required"] and c["status"] != "ok" for c in checks)
    return {
        "schema_version": 2, "profile": profile, "platform": sys.platform,
        "runtime": {
            "executable": os.path.abspath(sys.executable) if sys.executable else None,
            "version": ".".join(map(str, sys.version_info[:3])),
        },
        "ok": required_failures == 0, "required_failures": required_failures,
        "optional_issues": optional_issues, "checks": checks,
        "limitations": (
            "Prerequisite queries only: no build, game, browser, audio device or gameplay validation. "
            "Native linker/library queries use the host toolchain; custom Cargo target/linker overrides are not validated. "
            "Suggested commands are not executed."
        ),
    }


def render_human(report):
    """Format the same checks and actions exposed by --json, without ANSI color."""
    lines = [f"Ferravex environment doctor — {report['profile']} ({report['platform']})"]
    for check in report["checks"]:
        role = "required" if check["required"] else "optional"
        lines.append(f"[{check['status'].upper()}] {check['name']} ({role}): {check['detail']}")
        if check.get("executable"):
            lines.append(f"  Executable: {_summary(check['executable'])}")
        if check.get("version"):
            lines.append(f"  Version: {check['version']}")
        if check.get("probe") and check["probe"]["failure"]:
            lines.append(f"  Probe failure: {check['probe']['failure']}")
        if check["action"]:
            lines.append(f"  Action: {check['action']}")
    if report.get("runtime"):
        runtime = report["runtime"]
        lines.append(f"Doctor interpreter: {_summary(runtime['executable'] or '(unknown)')} (Python {runtime['version']})")
    lines.extend(("", f"{report['required_failures']} required failures; {report['optional_issues']} optional issues.",
                  "Required prerequisites satisfied." if report["ok"] else "Required prerequisites are incomplete.",
                  "Use --profile core, web, or native to select a narrower workflow.", report["limitations"]))
    return "\n".join(lines)


def main(argv=None):
    """Print a human/JSON report and return its documented process exit code."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--json", action="store_true", help="emit only the versioned JSON report")
    parser.add_argument("--profile", choices=PROFILES, default="all", help="required workflow (default: all)")
    parser.add_argument("--timeout", type=float, default=5.0, help="positive time limit per query, in seconds (default: 5)")
    args = parser.parse_args(argv)
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be positive and finite")
    try:
        report = collect_report(args.profile, args.timeout)
    except KeyboardInterrupt:
        print("Doctor interrupted.", file=sys.stderr)
        return 130
    print(json.dumps(report, indent=2, ensure_ascii=True) if args.json else render_human(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
