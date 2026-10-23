#!/usr/bin/env python3
"""Test the verifier without builds; --verify builds fresh 2D and 3D scaffolds.

Python 3.11+, standard library only. Real verification keeps projects, logs and
report.json in a unique target/journey-200/template-parity/run-* directory.
No repository manifests, templates, lockfile or shared dist are written.
"""

import argparse
import hashlib
from html.parser import HTMLParser
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
import unittest


ROOT = Path(__file__).resolve().parent.parent
WASM = "wasm32-unknown-unknown"
WASM_EXPORTS = {
    "memory": 2,
    "ferravex_init": 0,
    "ferravex_frame": 0,
    "ferravex_key": 0,
    "ferravex_focus": 0,
    "ferravex_feedback_preferences": 0,
}
LIMITATIONS = [
    "Native binaries are linked, not launched in a window.",
    "Wasm is compiled and its export table inspected, not instantiated or rendered.",
    "Input tests inject controller samples; no physical controller, keyboard or touch test.",
    "The generated async play loop is compiled; the headless harness calls public rules/input APIs.",
    "The templates have held movement and menu actions, no network/replay gameplay or feedback effects.",
    "No browser, GPU, audio, accessibility, performance or cross-platform determinism claim.",
]

# Extra acceptance tests live ONLY in each freshly generated project. The
# scaffold's own unit tests and rustdoc examples are also run without changes.
RUST_TESTS = r'''use @@CRATE@@::Game;
use ferravex::prelude::*;
use ferravex::render::input::{PadState, Preset, gamepad_sample, host_frame};

fn pad() -> PadState {
    PadState { connected: true, ..PadState::default() }
}

fn tap(state: &mut PadState, button: usize) {
    state.buttons[button] = 1.0;
    assert!(gamepad_sample(0, *state));
    state.buttons[button] = 0.0;
    assert!(gamepad_sample(0, *state));
}

#[test]
fn generated_lifecycle_uses_the_facade_and_real_goal() {
    let _native_entry: fn() = @@CRATE@@::run;
    let mut game = Game::new(false);
    let session: &GameSession = game.session();
    assert_eq!(session.state(), SessionState::Ready);
    assert_eq!(game.advance(100.0, Vec2::X).steps, 0);
    assert!(game.start());
    let step = 1.0 / Game::TICK_RATE_HZ;
    assert_eq!(game.advance(step, Vec2::X).steps, 1);
    let pose = game.position();
    assert!(game.toggle_pause());
    assert_eq!(game.advance(100.0, Vec2::X).steps, 0);
    assert_eq!(game.render_position(0.0), pose);
    assert!(game.toggle_pause());
    assert_eq!(game.advance(0.0, Vec2::X).steps, 0);
    for _ in 0..(3.0 * Game::TICK_RATE_HZ).ceil() as usize {
        game.advance(step, Vec2::X);
    }
    assert_eq!(game.session().state(), SessionState::Won);
    assert_eq!(game.advance(100.0, -Vec2::X).steps, 0);
    game.restart();
    assert_eq!(game.session().tick(), 0);
    assert_eq!(game.session().state(), SessionState::Playing);
    assert_eq!(game.position(), vec2(-4.0, 0.0));
    assert_eq!(game.render_position(0.5), game.position());
}

#[test]
fn generated_motion_matches_at_30_60_and_144_render_frames() {
    let mut poses = Vec::new();
    for frames in [30, 60, 144] {
        let mut game = Game::new(true);
        for _ in 0..frames {
            game.advance((60.0 / f64::from(frames)) / Game::TICK_RATE_HZ,
                         vec2(0.25, 0.125));
        }
        assert_eq!(game.session().tick(), 60);
        poses.push(game.position());
    }
    assert_eq!(poses[0], poses[1]);
    assert_eq!(poses[1], poses[2]);
}

#[test]
fn explore_preset_moves_the_game_and_retains_edges_until_a_tick() {
    let mut game = Game::new(false);
    let mut input = PlayerInput::preset("journey200-185-input", Preset::Explore);
    let mut state = pad();
    assert!(gamepad_sample(0, state));
    host_frame(0.0, 640, 360, || input.sample(false)); // neutral reconnect
    tap(&mut state, 0); // shipped global confirm binding
    host_frame(0.0, 640, 360, || {
        input.sample(false);
        assert!(input.pressed("confirm"));
        assert!(!input.pressed("confirm"));
        assert!(game.start());
    });
    host_frame(0.0, 640, 360, || input.sample(true));
    state.axes[0] = 1.0;
    assert!(gamepad_sample(0, state));
    tap(&mut state, 2); // Explore's interact; a future gameplay extension
    let short = (0.25 / Game::TICK_RATE_HZ) as f32;
    host_frame(short, 640, 360, || {
        input.sample(true);
        let frame = game.advance(f64::from(input.simulation_dt(short)), input.movement());
        assert_eq!(frame.steps, 0); // deliberately do not consume interact
    });
    let catch_up = (2.0 / Game::TICK_RATE_HZ) as f32;
    host_frame(catch_up, 640, 360, || {
        input.sample(true);
        let frame = game.advance(f64::from(input.simulation_dt(catch_up)), input.movement());
        assert_eq!(frame.steps, 2);
        let mut presses = 0;
        let mut releases = 0;
        for _ in 0..frame.steps {
            let [interact, same] = input.take_tick(["interact", "interact"]);
            assert_eq!(interact, same);
            presses += u32::from(interact.pressed);
            releases += u32::from(interact.released);
        }
        assert_eq!((presses, releases), (1, 1));
        assert!(game.position().x > -4.0);
    });
    tap(&mut state, 9); // shipped global pause binding
    host_frame(0.0, 640, 360, || {
        input.sample(true);
        assert!(input.pressed("pause"));
        assert!(game.toggle_pause());
        assert_eq!(game.advance(10.0, input.movement()).steps, 0);
    });
    tap(&mut state, 8); // restart stays global while gameplay is disabled
    host_frame(0.0, 640, 360, || {
        input.sample(false);
        assert!(input.pressed("restart"));
        game.restart();
        assert_eq!(game.session().tick(), 0);
        assert_eq!(game.render_position(0.5), vec2(-4.0, 0.0));
    });
}

#[test]
fn controls_gate_local_time_but_do_not_own_an_authoritative_clock() {
    let mut input = PlayerInput::transient("journey200-185-menu");
    let mut local = Game::new(true);
    let mut authority = GameSession::new(60.0, 8, true);
    host_frame(0.0, 640, 360, || input.sample(true));
    input.set_controls_open(true);
    host_frame(1.0 / 60.0, 640, 360, || {
        input.sample(true);
        assert!(!input.gameplay_active());
        assert_eq!(input.simulation_dt(1.0 / 60.0), 0.0);
        assert_eq!(local.advance(f64::from(input.simulation_dt(1.0 / 60.0)), Vec2::X).steps, 0);
        assert_eq!(authority.advance(1.0 / 60.0).steps, 1);
    });
    input.set_controls_open(false);
    host_frame(0.0, 640, 360, || {
        input.sample(true);
        assert_eq!(input.simulation_dt(f32::NAN), 0.0);
        assert_eq!(input.simulation_dt(-1.0), 0.0);
        assert_eq!(input.simulation_dt(100.0), 0.1);
        assert_eq!(local.advance(0.0, Vec2::X).steps, 0);
    });
}
'''


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def wasm_exports(data):
    """Inspect a bounded Wasm v1 export table; this is not module validation."""
    require(len(data) <= 64 * 1024 * 1024, "Wasm exceeds 64 MiB inspection limit")
    require(data[:8] == b"\0asm\x01\0\0\0", "missing Wasm v1 header")

    def uint(offset, end):
        value = 0
        for shift in range(0, 35, 7):
            require(offset < end, "truncated Wasm integer")
            byte = data[offset]
            offset += 1
            require(shift < 28 or byte < 16, "overflowing Wasm integer")
            value |= (byte & 127) << shift
            if byte < 128:
                return value, offset
        raise ValueError("overlong Wasm integer")

    offset, exports, found = 8, {}, False
    while offset < len(data):
        section = data[offset]
        size, offset = uint(offset + 1, len(data))
        end = offset + size
        require(end <= len(data), "truncated Wasm section")
        if section == 7:
            require(not found, "duplicate Wasm export section")
            found = True
            count, offset = uint(offset, end)
            require(count <= 4096, "too many Wasm exports")
            for _ in range(count):
                length, offset = uint(offset, end)
                require(offset + length < end, "truncated Wasm export")
                name = data[offset:offset + length].decode("utf-8")
                offset += length
                kind = data[offset]
                _, offset = uint(offset + 1, end)
                require(kind <= 4 and name not in exports, "invalid/duplicate Wasm export")
                exports[name] = kind
            require(offset == end, "trailing bytes in Wasm export section")
        offset = end
    require(found, "missing Wasm export section")
    return exports


class HostHTML(HTMLParser):
    def __init__(self):
        super().__init__()
        self.references = []
        self.games = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        self.references.extend(attrs[key] for key in ("src", "href") if attrs.get(key))
        if tag == "canvas":
            self.games.append(attrs.get("data-game"))


def inspect_host(project, package, profile):
    """Check fresh stage identity and literal local resources, without a browser."""
    stage = project / "target/web"
    filename = package.replace("-", "_") + ".wasm"
    folder = "debug" if profile == "dev" else "release"
    built = project / "target" / WASM / folder / filename
    staged = stage / filename
    require(built.is_file() and staged.is_file(), "built/staged Wasm is missing")
    require(digest(built) == digest(staged), "staged Wasm differs from fresh build")
    with staged.open("rb") as stream:
        exports = wasm_exports(stream.read(64 * 1024 * 1024 + 1))
    for name, kind in WASM_EXPORTS.items():
        require(exports.get(name) == kind, f"missing/wrong Wasm export: {name}")
    page = HostHTML()
    page.feed((stage / "index.html").read_text(encoding="utf-8"))
    require(page.games == [package], "canvas game ID does not match Wasm name")
    catalog = json.loads((stage / "games.json").read_text(encoding="utf-8"))
    require([row["id"] for row in catalog] == [package], "generated guide game ID mismatch")
    references = [(stage, ref) for ref in page.references]
    for path in stage.rglob("*.js"):
        # Includes current static/dynamic imports, AudioWorklet and games.json.
        # Not a JS parser: computed paths are outside this static check.
        references.extend((path.parent, match[1]) for match in re.findall(
            r'''(['"])(\./[^'"\s]+\.(?:js|json))\1''', path.read_text(encoding="utf-8")))
    for parent, reference in references:
        path = (parent / reference).resolve()
        require(path.is_relative_to(stage.resolve()) and path.is_file(),
                f"missing or nonlocal host resource: {reference}")
    for filename in ("runtime.js", "audio-worklet.js", "retained-shader.js", "effects-shader.js"):
        require((stage / filename).is_file(), f"missing host resource: {filename}")
    return dict(wasm=str(staged), sha256=digest(staged), bytes=staged.stat().st_size,
                required_exports=sorted(WASM_EXPORTS), local_references=len(references))


def inspect_project(project, package, template):
    manifest = tomllib.loads((project / "Cargo.toml").read_text(encoding="utf-8"))
    config = json.loads((project / "ferravex.json").read_text(encoding="utf-8"))
    require("workspace" in manifest, "generated project is not a standalone workspace")
    require(manifest["package"]["name"] == package, "Cargo package mismatch")
    require(config["schema_version"] == 1 and config["package"] == package
            and config["template"] == template.upper(), "generation manifest mismatch")
    require(Path(config["engine_root"]).resolve() == ROOT, "wrong engine checkout")
    actual = sorted(path.relative_to(project).as_posix()
                    for path in project.rglob("*") if path.is_file())
    require(config["generated_files"] == actual, "generation manifest file list mismatch")
    return dict(tick_rate=config["tick_rate"], title=config["window_title"],
                generated_files=actual)


def inspect_metadata(metadata, project, package):
    require(Path(metadata["workspace_root"]).resolve() == project, "Cargo workspace escaped project")
    require(Path(metadata["target_directory"]).resolve() == project / "target",
            "Cargo target directory escaped project")
    members = {row["id"] for row in metadata["packages"] if row["name"] == package}
    require(set(metadata["workspace_members"]) == members and len(members) == 1,
            "generated workspace includes unrelated packages")
    for row in metadata["packages"]:
        path = Path(row["manifest_path"]).resolve()
        require(row.get("source") is None and (path == project / "Cargo.toml"
                or path.is_relative_to(ROOT / "crates")),
                f"dependency is not a checkout-owned path package: {row['name']}")
    return dict(packages=sorted(row["name"] for row in metadata["packages"]))


def run_command(command, cwd, log, env, timeout):
    """Run argv with bounded time and file-backed output, preserving first failure."""
    started = time.monotonic()
    result = dict(command=command, cwd=str(cwd), log=str(log), status="failed")
    with log.open("x", encoding="utf-8") as output:
        try:
            with subprocess.Popen(command, cwd=cwd, env=env, stdout=output,
                                  stderr=subprocess.STDOUT, start_new_session=os.name == "posix") as child:
                try:
                    result["returncode"] = child.wait(timeout=timeout)
                except (subprocess.TimeoutExpired, KeyboardInterrupt):
                    if os.name == "posix":
                        os.killpg(child.pid, signal.SIGKILL)
                    else:
                        child.kill()
                    child.wait()
                    raise
            result["status"] = "passed" if result["returncode"] == 0 else "failed"
        except (OSError, subprocess.TimeoutExpired) as error:
            result["error"] = str(error)
            output.write(str(error) + "\n")
    result["seconds"] = round(time.monotonic() - started, 3)
    return result


def verify(timeout, profile):
    root = ROOT / "target/journey-200/template-parity"
    root.mkdir(parents=True, exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="run-", dir=root)).resolve()
    report = dict(schema_version=1, scope="generated-2d-3d", ok=False, run_directory=str(run),
                  web_profile=profile, limitations=LIMITATIONS, projects=[], prerequisites=[])
    report_path = run / "report.json"

    def save():
        report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print(f"Template parity evidence: {run}", flush=True)
    save()
    env = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_NET_OFFLINE="true",
               PYTHONDONTWRITEBYTECODE="1", CARGO_TERM_COLOR="never")
    probe = run_command(["rustc", "-vV"], ROOT, run / "rustc.log", env, timeout)
    report["prerequisites"].append(probe)
    if probe["status"] != "passed":
        save()
        return 1
    version = (run / "rustc.log").read_text(encoding="utf-8")
    host = re.search(r"^host: (\S+)$", version, re.M)
    if host is None:
        report["error"] = "rustc -vV did not report a native host triple"
        save()
        return 1
    report["rustc"] = version.strip()
    # Always name the native target, overriding an inherited Wasm build target.
    native = host[1]
    for template in ("2d", "3d"):
        package = f"journey200-parity-{template}"
        # Exercise spaces/quotes with real Cargo and helper argv, not a shell.
        project = run / f"{template} game 'quoted'"
        logs = run / f"logs-{template}"
        logs.mkdir()
        project_env = dict(env, CARGO_TARGET_DIR=str(project / "target"),
                           XDG_CONFIG_HOME=str(project / "target/config"),
                           APPDATA=str(project / "target/config"))
        row = dict(template=template, package=package, project=str(project), checks=[], status="failed")
        report["projects"].append(row)

        def command(name, argv):
            result = run_command(argv, project if project.is_dir() else run,
                                 logs / f"{name}.log", project_env, timeout)
            result["name"] = name
            row["checks"].append(result)
            save()
            print(f"{template}: {name}: {result['status']} ({result['log']})", flush=True)
            return result["status"] == "passed"

        def cargo(verb, *args):
            return ["cargo", verb, "--manifest-path", str(project / "Cargo.toml"),
                    "--offline", "--locked", *args]

        try:
            argv = [sys.executable, "-B", str(ROOT / "scripts/new-game.py"), package,
                    str(project), "--template", template]
            if template == "3d":
                argv += ["--tick-rate", "59.94", "--window-title", 'Parity "3D" & café']
            if not command("generate", argv):
                continue
            row["generation"] = inspect_project(project, package, template)
            lock_hash = digest(project / "Cargo.lock")
            if not command("metadata", cargo("metadata", "--format-version", "1")):
                continue
            row["dependencies"] = inspect_metadata(
                json.loads((logs / "metadata.log").read_text(encoding="utf-8")), project, package)
            (project / "tests").mkdir()
            (project / "tests/journey200_185.rs").write_text(
                RUST_TESTS.replace("@@CRATE@@", package.replace("-", "_")), encoding="utf-8")
            common = ["-p", package, "--target", native, "--target-dir", str(project / "target"), "-j", "2"]
            if not command("headless-tests-and-docs", cargo("test", *common)):
                continue
            if not command("native-build", cargo("build", *common, "--bin", package)):
                continue
            binary = project / "target" / native / "debug" / (package + (".exe" if "windows" in native else ""))
            require(binary.is_file(), "successful native build did not produce the expected binary")
            row["native"] = dict(path=str(binary), sha256=digest(binary), bytes=binary.stat().st_size)
            if not command("web-helper-help", [sys.executable, "-B", str(project / "scripts/build-web.py"), "--help"]):
                continue
            if not command("wasm-build-and-stage", [sys.executable, "-B", str(project / "scripts/build-web.py"), "--profile", profile]):
                continue
            row["web"] = inspect_host(project, package, profile)
            require(digest(project / "Cargo.lock") == lock_hash, "generated lock changed under --locked")
            row["status"] = "passed"
        except (OSError, ValueError, KeyError, TypeError) as error:
            row["error"] = str(error)
            print(f"{template}: inspection failed: {error}", file=sys.stderr, flush=True)
        finally:
            save()
        # A failure records the first failing phase for this project; the other
        # dimension still supplies independent evidence. Never retry a failure.
    report["ok"] = len(report["projects"]) == 2 and all(
        row["status"] == "passed" for row in report["projects"])
    save()
    print(f"{'PASS' if report['ok'] else 'FAIL'}: {report_path}", flush=True)
    return 0 if report["ok"] else 1


class VerifierTests(unittest.TestCase):
    def host_fixture(self, root, *, profile="dev", exports=None):
        # Synthetic bytes are ONLY verifier fixtures, never build evidence.
        exports = WASM_EXPORTS if exports is None else exports

        def uint(value):
            result = bytearray()
            while value >= 128:
                result.append((value & 127) | 128)
                value >>= 7
            result.append(value)
            return bytes(result)

        table = uint(len(exports)) + b"".join(
            uint(len(name)) + name.encode() + bytes([kind, 0]) for name, kind in exports.items())
        data = b"\0asm\x01\0\0\0\x07" + uint(len(table)) + table
        built = root / "target" / WASM / ("debug" if profile == "dev" else "release") / "test_game.wasm"
        stage = root / "target/web"
        built.parent.mkdir(parents=True)
        stage.mkdir(parents=True)
        built.write_bytes(data)
        (stage / built.name).write_bytes(data)
        (stage / "index.html").write_text('<canvas data-game="test-game"></canvas><script src="./runtime.js"></script>')
        (stage / "games.json").write_text('[{"id":"test-game"}]')
        (stage / "runtime.js").write_text("import './retained-shader.js'; import('./effects-shader.js'); new URL('./games.json', import.meta.url);")
        for name in ("retained-shader.js", "effects-shader.js", "audio-worklet.js"):
            (stage / name).write_text("")
        return stage

    def test_host_inspection_accepts_both_profile_paths(self):
        for profile in ("dev", "release"):
            with self.subTest(profile=profile), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                stage = self.host_fixture(root, profile=profile)
                report = inspect_host(root, "test-game", profile)
                self.assertEqual(report["sha256"], digest(stage / "test_game.wasm"))
                self.assertEqual(report["local_references"], 4)

    def test_host_inspection_rejects_stale_stage_missing_resources_and_wrong_game(self):
        for defect in ("stale", "missing", "escape", "wrong-game"):
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                stage = self.host_fixture(root)
                if defect == "stale":
                    (stage / "test_game.wasm").write_bytes(b"old")
                elif defect == "missing":
                    (stage / "effects-shader.js").unlink()
                elif defect == "escape":
                    (stage / "runtime.js").write_text("import './../escape.js'")
                    (stage.parent / "escape.js").write_text("")
                else:
                    (stage / "games.json").write_text('[{"id":"other"}]')
                with self.assertRaises(ValueError):
                    inspect_host(root, "test-game", "dev")

    def test_host_inspection_rejects_missing_and_wrong_kind_abi(self):
        for kind in (None, 2):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                exports = dict(WASM_EXPORTS)
                if kind is None:
                    del exports["ferravex_init"]
                else:
                    exports["ferravex_init"] = kind
                self.host_fixture(root, exports=exports)
                with self.assertRaisesRegex(ValueError, "ferravex_init"):
                    inspect_host(root, "test-game", "dev")

    def test_export_reader_rejects_malformed_or_missing_exports(self):
        header = b"\0asm\x01\0\0\0"
        for data in (b"", header, header + b"\x07\x80", header + b"\x07\x05\0",
                     header + b"\x07\x80\x80\x80\x80\x10", header + b"\x07\x02\0\0"):
            with self.subTest(data=data), self.assertRaises(ValueError):
                wasm_exports(data)

    def test_export_reader_checks_names_kinds_and_duplicates(self):
        header = b"\0asm\x01\0\0\0"
        entry = b"\x06memory\x02\0"
        data = header + bytes([7, len(entry) + 1, 1]) + entry
        self.assertEqual(wasm_exports(data), {"memory": 2})
        with self.assertRaisesRegex(ValueError, "duplicate"):
            wasm_exports(header + bytes([7, 2 * len(entry) + 1, 2]) + entry * 2)

    def test_command_failure_and_timeout_are_not_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            failed = run_command([sys.executable, "-c", "raise SystemExit(7)"], root,
                                 root / "failed.log", dict(os.environ), 5)
            self.assertEqual((failed["status"], failed["returncode"]), ("failed", 7))
            timeout = run_command([sys.executable, "-c", "import time; time.sleep(5)"], root,
                                  root / "timeout.log", dict(os.environ), 0.05)
            self.assertEqual(timeout["status"], "failed")
            self.assertIn("timed out", timeout["error"])

    def test_missing_executable_is_a_logged_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = run_command([str(root / "absent")], root, root / "missing.log", dict(os.environ), 5)
            self.assertEqual(result["status"], "failed")
            self.assertTrue((root / "missing.log").read_text())

    def test_metadata_rejects_foreign_dependencies_and_shared_workspace(self):
        project = ROOT / "target/fake-parity-project"
        meta = dict(workspace_root=str(project), target_directory=str(project / "target"),
                    workspace_members=["game"], packages=[dict(id="game", name="game", source=None,
                    manifest_path=str(project / "Cargo.toml"))])
        self.assertEqual(inspect_metadata(meta, project, "game"), {"packages": ["game"]})
        meta["packages"].append(dict(id="foreign", name="foreign", source="registry+test",
                                     manifest_path=str(ROOT / "crates/foreign/Cargo.toml")))
        with self.assertRaisesRegex(ValueError, "checkout-owned"):
            inspect_metadata(meta, project, "game")
        meta["workspace_root"] = str(ROOT)
        with self.assertRaisesRegex(ValueError, "workspace escaped"):
            inspect_metadata(meta, project, "game")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true", help="generate both projects and run native tests/builds plus Wasm staging")
    parser.add_argument("--web-profile", choices=("dev", "release"), default="dev",
                        help="generated web helper profile (default: dev)")
    parser.add_argument("--timeout", type=float, default=600, help="seconds per child command, 1..1800 (default: 600)")
    args = parser.parse_args(argv)
    if not math.isfinite(args.timeout) or not 1 <= args.timeout <= 1800:
        parser.error("--timeout must be finite and in 1..1800")
    if not args.verify:
        if args.web_profile != "dev" or args.timeout != 600:
            parser.error("--web-profile and --timeout require --verify")
        result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(VerifierTests))
        return 0 if result.wasSuccessful() else 1
    return verify(args.timeout, args.web_profile)


if __name__ == "__main__":
    raise SystemExit(main())
