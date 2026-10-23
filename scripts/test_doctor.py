"""Read-only environment checks must distinguish missing, failed and optional tools."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("doctor", Path(__file__).with_name("doctor.py"))
doctor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(doctor)


class DoctorTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ferravex doctor ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "Cargo.toml").write_text(
            '[workspace.package]\nrust-version = "1.88"\n\n[workspace.dependencies]\n', encoding="utf-8")
        self.wasm = self.root / "active compiler" / "wasm lib"
        self.wasm.mkdir(parents=True)
        for name in ("libstd-hash.rlib", "libcore-hash.rlib"):
            (self.wasm / name).write_bytes(b"test library")
        self.libs = self.root / "native libs"
        self.libs.mkdir()
        for name in ("libX11.so", "libGL.so", "libasound.so.2"):
            (self.libs / name).write_bytes(b"test library")
        self.available = {"rustc", "cargo", "python3", "node", "cc", "rustup", "git",
                          "rustfmt", "glxinfo", "xvfb-run", "xcrun"}
        self.outputs = {
            ("rustc", "--version"): "rustc 1.88.0 (test 2025-01-01)",
            ("cargo", "--version"): "cargo 1.88.0 (test 2025-01-01)",
            ("python3", "--version"): "Python 3.11.15",
            ("node", "--version"): "v24.0.0",
            ("rustc", "--print", "target-libdir", "--target", doctor.WASM_TARGET): str(self.wasm),
            ("cc", "--version"): "cc 14.1.0",
            ("rustup", "--version"): "rustup 1.28.0",
            ("git", "--version"): "git version 2.49.0",
            ("rustfmt", "--version"): "rustfmt 1.8.0",
            ("cargo", "clippy", "--version"): "clippy 0.1.88",
            ("glxinfo", "-B"): "OpenGL renderer string: test renderer\nOpenGL core profile version string: 4.6 Mesa\n",
        }
        for name in ("libX11.so", "libGL.so", "libasound.so.2"):
            self.outputs[("cc", f"-print-file-name={name}")] = str(self.libs / name)
        self.calls = []
        for patcher in (
            patch.object(doctor, "ROOT", self.root),
            patch.object(doctor.sys, "platform", "linux"),
            patch.object(doctor.shutil, "which", side_effect=self.which),
            patch.object(doctor, "_run_bounded", side_effect=self.run_query),
            patch.dict(os.environ, {"PATH": "/mock/bin", "DISPLAY": ":99", "SENTINEL": "unchanged"}, clear=True),
        ):
            patcher.start()
            self.addCleanup(patcher.stop)

    def which(self, name):
        return os.path.abspath(f"/mock/bin/{name}") if name in self.available else None

    def run_query(self, command, **options):
        self.calls.append((command, options))
        self.assertTrue(Path(command[0]).is_absolute())
        logical_command = (Path(command[0]).name, *command[1:])
        # Unknown commands are a test failure, including accidental installs,
        # cargo builds, target add, package-manager queries, or shell commands.
        self.assertIn(logical_command, self.outputs)
        result = self.outputs[logical_command]
        if isinstance(result, Exception):
            raise result
        if isinstance(result, subprocess.CompletedProcess):
            return result
        return subprocess.CompletedProcess(command, 0, stdout=result, stderr="")

    def check(self, report, check_id):
        return next(check for check in report["checks"] if check["id"] == check_id)

    def test_healthy_full_environment(self):
        report = doctor.collect_report()
        self.assertTrue(report["ok"])
        self.assertEqual(report["required_failures"], 0)
        self.assertEqual(report["optional_issues"], 1)  # Xvfb presence is not execution evidence.
        self.assertEqual(self.check(report, "native.glx")["detail"], "OpenGL core 4.6; test renderer")
        self.assertEqual(report, json.loads(json.dumps(report)))
        ids = [c["id"] for c in report["checks"]]
        self.assertEqual(len(ids), len(set(ids)))

    def test_missing_every_tool_produces_report_without_execution(self):
        self.available.clear()
        report = doctor.collect_report()
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "rust.compiler")["status"], "missing")
        self.assertEqual(self.check(report, "rust.wasm")["status"], "unverified")
        self.assertEqual(self.check(report, "native.x11")["status"], "unverified")
        self.assertEqual(self.calls, [])
        for check in report["checks"]:
            if check["status"] != "ok":
                self.assertTrue(check["action"], check["id"])

    def test_optional_missing_tools_do_not_fail_core(self):
        self.available = {"rustc", "cargo", "python3", "cc"}
        self.outputs[("cargo", "clippy", "--version")] = subprocess.CompletedProcess(
            [], 101, stdout="", stderr="no such command: clippy")
        os.environ.pop("DISPLAY")
        (self.wasm / "libstd-hash.rlib").unlink()
        (self.libs / "libGL.so").unlink()
        report = doctor.collect_report("core")
        self.assertTrue(report["ok"])
        self.assertGreater(report["optional_issues"], 0)
        self.assertFalse(self.check(report, "node")["required"])
        self.assertFalse(self.check(report, "native.glx")["required"])
        self.assertEqual(self.check(report, "tool.clippy")["status"], "error")

    def test_web_does_not_require_native_linker_libraries_or_display(self):
        self.available.remove("cc")
        os.environ.pop("DISPLAY")
        report = doctor.collect_report("web")
        self.assertTrue(report["ok"])
        for check_id in ("native.linker", "native.x11", "native.display", "native.glx"):
            self.assertFalse(self.check(report, check_id)["required"])
        self.assertTrue(self.check(report, "rust.wasm")["required"])
        self.assertTrue(self.check(report, "node")["required"])

    def test_unconfigured_clang_is_not_the_default_rust_linker(self):
        self.available.remove("cc")
        self.available.add("clang")
        report = doctor.collect_report("core")
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "native.linker")["status"], "missing")

    def test_empty_compiler_version_is_not_a_usable_toolchain(self):
        self.outputs[("cc", "--version")] = ""
        report = doctor.collect_report("core")
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "native.linker")["status"], "error")

    def test_native_does_not_require_node_or_wasm(self):
        self.available.remove("node")
        (self.wasm / "libstd-hash.rlib").unlink()
        report = doctor.collect_report("native")
        self.assertTrue(report["ok"])
        self.assertFalse(self.check(report, "rust.wasm")["required"])
        self.assertFalse(self.check(report, "node")["required"])

    def test_minimum_rust_version_follows_workspace_manifest(self):
        (self.root / "Cargo.toml").write_text(
            '[workspace.package]\nrust-version = "1.89.1"\n', encoding="utf-8")
        report = doctor.collect_report("core")
        self.assertFalse(report["ok"])
        for check_id in ("rust.compiler", "rust.cargo"):
            check = self.check(report, check_id)
            self.assertEqual(check["status"], "too_old")
            self.assertIn("1.89.1", check["detail"])

    def test_old_python_and_node_fail_web(self):
        self.outputs[("python3", "--version")] = "Python 3.9.20"
        self.outputs[("node", "--version")] = "v18.19.0"
        report = doctor.collect_report("web")
        self.assertEqual(report["required_failures"], 2)
        self.assertEqual(self.check(report, "python")["status"], "too_old")
        self.assertEqual(self.check(report, "node")["status"], "too_old")

    def test_python_without_stdlib_tomllib_fails_tooling(self):
        self.outputs[("python3", "--version")] = "Python 3.10.15"
        report = doctor.collect_report("web")
        self.assertFalse(report["ok"])
        check = self.check(report, "python")
        self.assertEqual(check["status"], "too_old")
        self.assertIn("3.11", check["action"])

    def test_malformed_and_empty_version_output_fail(self):
        for output in ("", "this is not rustc", "rustc unknown", "rustc 1.88"):
            with self.subTest(output=output):
                self.outputs[("rustc", "--version")] = output
                report = doctor.collect_report("web")
                self.assertFalse(report["ok"])
                self.assertEqual(self.check(report, "rust.compiler")["status"], "error")

    def test_wasm_requires_actual_active_compiler_libraries_not_only_printed_path(self):
        (self.wasm / "libcore-hash.rlib").unlink()
        report = doctor.collect_report("web")
        self.assertFalse(report["ok"])
        check = self.check(report, "rust.wasm")
        self.assertEqual(check["status"], "missing")
        self.assertIn("rustup target add wasm32-unknown-unknown", check["action"])
        self.assertEqual(check["command"], ["rustc", "--print", "target-libdir", "--target", doctor.WASM_TARGET])

    def test_wasm_rejects_empty_library_and_malformed_query(self):
        (self.wasm / "libcore-hash.rlib").write_bytes(b"")
        self.assertEqual(self.check(doctor.collect_report("web"), "rust.wasm")["status"], "missing")
        self.outputs[("rustc", "--print", "target-libdir", "--target", doctor.WASM_TARGET)] = "unexpected\noutput"
        self.assertEqual(self.check(doctor.collect_report("web"), "rust.wasm")["status"], "error")

    def test_rustup_is_not_required_with_complete_custom_toolchain(self):
        self.available.remove("rustup")
        report = doctor.collect_report("web")
        self.assertTrue(report["ok"])
        self.assertEqual(self.check(report, "rust.wasm")["status"], "ok")

    def test_target_directory_permission_error_is_reported(self):
        with patch.object(doctor.os, "scandir", side_effect=PermissionError("denied")):
            report = doctor.collect_report("web")
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "rust.wasm")["status"], "error")
        self.assertIn("denied", self.check(report, "rust.wasm")["detail"])

    def test_runtime_soname_does_not_replace_development_link_library(self):
        self.outputs[("cc", "-print-file-name=libGL.so")] = "libGL.so"
        (self.libs / "libGL.so.1").write_bytes(b"runtime only")
        report = doctor.collect_report("native")
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "native.opengl")["status"], "missing")
        self.assertIn("libgl1-mesa-dev", self.check(report, "native.opengl")["action"])

    def test_wayland_alone_is_not_an_x11_display(self):
        os.environ.pop("DISPLAY")
        os.environ["WAYLAND_DISPLAY"] = "wayland-0"
        report = doctor.collect_report("native")
        self.assertFalse(report["ok"])
        self.assertIn("XWayland", self.check(report, "native.display")["detail"])
        self.assertEqual(self.check(report, "native.glx")["status"], "unverified")
        self.assertNotIn([os.path.abspath("/mock/bin/glxinfo"), "-B"], [command for command, _ in self.calls])

    def test_display_variable_does_not_mask_glx_connection_failure(self):
        self.outputs[("glxinfo", "-B")] = subprocess.CompletedProcess(
            [], 1, stdout="", stderr="Error: unable to open display :99")
        report = doctor.collect_report("native")
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "native.display")["status"], "ok")
        self.assertEqual(self.check(report, "native.glx")["status"], "error")
        self.assertIn("unable to open display", self.check(report, "native.glx")["detail"])

    def test_glx_requires_desktop_core_profile_at_least_33(self):
        for output, status in (
            ("OpenGL core profile version string: 3.2 Mesa", "too_old"),
            ("OpenGL version string: 4.6 (Compatibility Profile) Mesa", "unverified"),
            ("OpenGL ES profile version string: OpenGL ES 3.3", "unverified"),
            ("OpenGL core profile version string: 3.3 Mesa", "ok"),
        ):
            with self.subTest(output=output):
                self.outputs[("glxinfo", "-B")] = output
                report = doctor.collect_report("native")
                self.assertEqual(self.check(report, "native.glx")["status"], status)
                self.assertEqual(report["ok"], status == "ok")

    def test_unavailable_glx_probe_is_not_a_native_pass(self):
        self.available.remove("glxinfo")
        report = doctor.collect_report("native")
        self.assertFalse(report["ok"])
        self.assertEqual(self.check(report, "native.glx")["status"], "missing")
        self.assertIn("mesa-utils", self.check(report, "native.glx")["action"])

    def test_query_failures_are_bounded_and_do_not_abort_remaining_checks(self):
        for failure, expected in (
            (subprocess.TimeoutExpired(["rustc"], 0.25), "timed out"),
            (PermissionError("permission denied"), "permission denied"),
            (FileNotFoundError("removed after PATH lookup"), "Cannot execute/read resolved query"),
            (subprocess.CompletedProcess([], 127, stdout="", stderr="loader missing"), "exited 127"),
        ):
            with self.subTest(failure=failure):
                self.outputs[("rustc", "--version")] = failure
                report = doctor.collect_report("web", timeout=0.25)
                self.assertFalse(report["ok"])
                self.assertEqual(self.check(report, "rust.compiler")["status"], "error")
                self.assertIn(expected, self.check(report, "rust.compiler")["detail"])
                self.assertEqual(self.check(report, "node")["status"], "ok")
                self.assertEqual(self.check(report, "rust.wasm")["status"], "unverified")

    def test_manifest_errors_are_required_failures(self):
        for manifest in ("", '[package]\nrust-version = "1.88"\n'):
            (self.root / "Cargo.toml").write_text(manifest, encoding="utf-8")
            report = doctor.collect_report("core")
            self.assertFalse(report["ok"])
            self.assertEqual(self.check(report, "workspace.manifest")["status"], "error")
        (self.root / "Cargo.toml").unlink()
        self.assertFalse(doctor.collect_report("core")["ok"])

    def test_read_only_commands_timeouts_and_environment_are_preserved(self):
        before = dict(os.environ)
        files_before = {str(path.relative_to(self.root)): path.read_bytes()
                        for path in self.root.rglob("*") if path.is_file()}
        doctor.collect_report(timeout=0.5)
        self.assertEqual(dict(os.environ), before)
        files_after = {str(path.relative_to(self.root)): path.read_bytes()
                       for path in self.root.rglob("*") if path.is_file()}
        self.assertEqual(files_before, files_after)
        for command, options in self.calls:
            self.assertIsInstance(command, list)
            self.assertFalse(options["shell"])
            self.assertEqual(options["cwd"], self.root)
            self.assertEqual(options["timeout"], 0.5)
            self.assertEqual(options["stdin"], subprocess.DEVNULL)
            self.assertEqual(options["env"]["RUSTUP_AUTO_INSTALL"], "0")
            self.assertEqual(options["env"]["CARGO_NET_OFFLINE"], "true")
            self.assertEqual(options["env"]["SENTINEL"], "unchanged")
            self.assertEqual(options["env"]["PYTHONDONTWRITEBYTECODE"], "1")
            self.assertEqual(options["env"]["RUSTUP_NO_UPDATE_CHECK"], "1")

    def test_json_stdout_and_exit_codes_match_required_failures(self):
        for missing, expected_exit in ((None, 0), ("git", 0), ("rustc", 1)):
            with self.subTest(missing=missing):
                if missing:
                    self.available.remove(missing)
                stdout, stderr = io.StringIO(), io.StringIO()
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    code = doctor.main(["--profile", "web", "--json"])
                report = json.loads(stdout.getvalue())
                self.assertEqual(code, expected_exit)
                self.assertEqual(report["ok"], code == 0)
                self.assertEqual(report["schema_version"], 2)
                self.assertEqual(stderr.getvalue(), "")

    def test_human_report_has_required_optional_and_actionable_remediation(self):
        self.available.remove("node")
        report = doctor.collect_report("web")
        output = doctor.render_human(report)
        self.assertIn("Node browser tests (required)", output)
        self.assertIn("Rust formatter (optional)", output)
        self.assertIn("Action: Install Node.js", output)
        self.assertIn("1 required failures", output)
        self.assertIn("not executed", output)

    def test_invalid_cli_options_have_usage_exit(self):
        for arguments in (["--profile", "wrong"], ["--timeout", "0"], ["--timeout", "-1"],
                          ["--timeout", "nan"], ["--timeout", "inf"]):
            with self.subTest(arguments=arguments), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    doctor.main(arguments)
                self.assertEqual(error.exception.code, 2)
        self.assertEqual(self.calls, [])

    def test_collect_report_rejects_invalid_arguments(self):
        with self.assertRaises(ValueError):
            doctor.collect_report("bad")
        for timeout in (0, -1, float("inf"), float("nan")):
            with self.assertRaises(ValueError):
                doctor.collect_report(timeout=timeout)

    def test_interruption_is_not_a_success(self):
        with patch.object(doctor, "collect_report", side_effect=KeyboardInterrupt):
            with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(doctor.main(["--json"]), 130)
            self.assertEqual(output.getvalue(), "")

    def test_macos_queries_sdk_without_linux_display_assumptions(self):
        sdk = self.root / "macOS SDK"
        for name in ("AppKit", "OpenGL"):
            (sdk / "System/Library/Frameworks" / f"{name}.framework").mkdir(parents=True)
        self.outputs[("xcrun", "--no-cache", "--sdk", "macosx", "--show-sdk-path")] = str(sdk)
        with patch.object(doctor.sys, "platform", "darwin"):
            report = doctor.collect_report("native")
        self.assertTrue(report["ok"])
        self.assertEqual(self.check(report, "native.sdk")["status"], "ok")
        self.assertEqual(self.check(report, "native.graphics")["status"], "unverified")
        self.assertNotIn("native.display", [c["id"] for c in report["checks"]])

    def test_macos_missing_and_malformed_sdk_are_reported(self):
        command = ("xcrun", "--no-cache", "--sdk", "macosx", "--show-sdk-path")
        with patch.object(doctor.sys, "platform", "darwin"):
            for output, expected in ((str(self.root / "missing SDK"), "missing"), ("", "error"),
                                     ("relative SDK", "error")):
                with self.subTest(output=output):
                    self.outputs[command] = output
                    report = doctor.collect_report("native")
                    self.assertFalse(report["ok"])
                    self.assertEqual(self.check(report, "native.sdk")["status"], expected)

    def test_windows_never_executes_unrelated_link_utility(self):
        self.available.add("link.exe")
        with patch.object(doctor.sys, "platform", "win32"):
            report = doctor.collect_report("native")
        self.assertFalse(report["ok"])  # PATH presence alone cannot prove a usable linker.
        self.assertEqual(self.check(report, "native.linker")["status"], "unverified")
        self.assertEqual(self.check(report, "native.graphics")["status"], "unverified")
        self.assertFalse(any(Path(command[0]).name == "link.exe" for command, _ in self.calls))

    def test_unsupported_native_host_does_not_break_web_profile(self):
        with patch.object(doctor.sys, "platform", "freebsd"):
            native = doctor.collect_report("native")
            web = doctor.collect_report("web")
        self.assertFalse(native["ok"])
        self.assertTrue(web["ok"])
        self.assertEqual(self.check(native, "native.host")["status"], "unsupported")


class ProvenanceTests(unittest.TestCase):
    setUp = DoctorTests.setUp
    which = DoctorTests.which
    run_query = DoctorTests.run_query
    check = DoctorTests.check

    def test_every_check_has_consistent_evidence_and_resolved_commands(self):
        report = doctor.collect_report("web", timeout=0.75)
        versions = {
            "rust.compiler": "1.88.0", "rust.cargo": "1.88.0", "python": "3.11.15",
            "node": "24.0.0", "native.linker": "cc 14.1.0", "native.glx": "4.6",
            "tool.rustup": "1.28.0", "tool.git": "2.49.0", "tool.rustfmt": "1.8.0",
            "tool.clippy": "0.1.88",
        }
        for check in report["checks"]:
            for field in ("executable", "version", "probe"):
                self.assertIn(field, check, check["id"])
            self.assertEqual(check["version"], versions.get(check["id"]))
            if check["probe"]:
                probe = check["probe"]
                self.assertEqual(check["executable"], os.path.abspath(f"/mock/bin/{check['command'][0]}"))
                self.assertEqual(probe["command"], [check["executable"], *check["command"][1:]])
                self.assertTrue(probe["attempted"])
                self.assertIsNone(probe["failure"])
                self.assertEqual(probe["returncode"], 0)
                self.assertEqual(probe["timeout_seconds"], 0.75)
                self.assertEqual(probe["output_limit_bytes"], 65536)
        self.assertEqual(report["runtime"]["executable"], os.path.abspath(sys.executable))
        self.assertEqual(report["runtime"]["version"], ".".join(map(str, sys.version_info[:3])))

    def test_missing_unusable_and_skipped_checks_have_distinct_evidence(self):
        self.available.remove("node")
        self.outputs[("rustc", "--version")] = FileNotFoundError("missing interpreter")
        report = doctor.collect_report("core")
        missing = self.check(report, "node")
        self.assertEqual(missing["status"], "missing")
        self.assertFalse(missing["required"])
        self.assertIsNone(missing["executable"])
        self.assertIsNone(missing["version"])
        self.assertEqual(missing["probe"]["failure"], "not_found")
        self.assertFalse(missing["probe"]["attempted"])
        self.assertEqual(missing["probe"]["command"], [])
        broken = self.check(report, "rust.compiler")
        self.assertEqual(broken["status"], "error")
        self.assertEqual(broken["executable"], os.path.abspath("/mock/bin/rustc"))
        self.assertEqual(broken["probe"]["failure"], "execution_error")
        self.assertTrue(broken["probe"]["attempted"])
        self.assertIsNone(broken["probe"]["returncode"])
        skipped = self.check(report, "rust.wasm")
        self.assertEqual(skipped["status"], "unverified")
        self.assertEqual(skipped["executable"], broken["executable"])
        self.assertIsNone(skipped["probe"])

    def test_partial_timeout_and_both_failure_streams_are_bounded_and_sanitized(self):
        for failure, reason in (
            (subprocess.TimeoutExpired([], 0.25, output=b"partial\x1b\xff", stderr=b"waiting\n"), "timeout"),
            (subprocess.CompletedProcess([], 23, stdout="output\x00" * 2000,
                                         stderr="failure\x1b" * 2000), "nonzero_exit"),
            (doctor._OutputLimitExceeded(b"x" * 65536, b""), "output_limit"),
        ):
            with self.subTest(reason=reason):
                self.outputs[("rustc", "--version")] = failure
                check = self.check(doctor.collect_report("web", timeout=0.25), "rust.compiler")
                self.assertEqual(check["status"], "error")
                self.assertEqual(check["probe"]["failure"], reason)
                self.assertEqual(check["probe"]["returncode"], 23 if reason == "nonzero_exit" else None)
                for text in (check["detail"], check["probe"]["stdout"], check["probe"]["stderr"]):
                    self.assertLessEqual(len(text), doctor.SUMMARY_LIMIT)
                    self.assertTrue(all(c.isprintable() for c in text))
                self.assertTrue(check["probe"]["stdout"])
                if reason != "output_limit":
                    self.assertTrue(check["probe"]["stderr"])

    def test_old_and_stderr_only_versions_retain_provenance(self):
        self.outputs[("python3", "--version")] = subprocess.CompletedProcess(
            [], 0, stdout="", stderr="Python 3.10.15\n")
        check = self.check(doctor.collect_report("web"), "python")
        self.assertEqual(check["status"], "too_old")
        self.assertEqual(check["version"], "3.10.15")
        self.assertEqual(check["executable"], os.path.abspath("/mock/bin/python3"))
        self.assertIsNone(check["probe"]["failure"])
        self.assertEqual(check["probe"]["stderr"], "Python 3.10.15")

    def test_extreme_version_components_do_not_abort_report(self):
        self.outputs[("rustc", "--version")] = "rustc " + "9" * 5000 + ".0.0"
        self.outputs[("glxinfo", "-B")] = "OpenGL core profile version string: " + "9" * 5000 + ".0"
        report = doctor.collect_report("native")
        self.assertTrue(report["ok"])
        for check_id in ("rust.compiler", "native.glx"):
            check = self.check(report, check_id)
            self.assertLessEqual(len(check["version"]), doctor.SUMMARY_LIMIT)
            self.assertIn("[truncated]", check["version"])

    def test_failed_compiler_does_not_launch_dependent_library_probes(self):
        self.outputs[("cc", "--version")] = PermissionError("denied")
        report = doctor.collect_report("web")
        self.assertTrue(report["ok"])
        for check_id in ("native.x11", "native.opengl", "native.alsa"):
            check = self.check(report, check_id)
            self.assertEqual(check["status"], "unverified")
            self.assertIsNone(check["probe"])
        self.assertFalse(any(command[1].startswith("-print-file-name") for command, _ in self.calls))

    def test_malformed_library_query_is_an_error_with_successful_probe_evidence(self):
        for output in ("", "bad\npath", "/embedded\0nul", "bad\rpath"):
            with self.subTest(output=output):
                self.outputs[("cc", "-print-file-name=libGL.so")] = output
                check = self.check(doctor.collect_report("native"), "native.opengl")
                self.assertEqual(check["status"], "error")
                self.assertIn("single library path", check["detail"])
                self.assertEqual(check["executable"], os.path.abspath("/mock/bin/cc"))
                self.assertEqual(check["probe"]["returncode"], 0)
                self.assertIsNone(check["probe"]["failure"])

    def test_wasm_uses_the_compiler_that_passed_the_version_check(self):
        with patch.object(doctor.shutil, "which", side_effect=[os.path.abspath("/first/rustc"), "/other/rustc"]):
            rust = doctor._tool("rust.compiler", "Rust", "rustc", True, "", 1)
            check = doctor._wasm_check(True, rust, 1)
        self.assertEqual(rust["executable"], os.path.abspath("/first/rustc"))
        self.assertEqual(check["executable"], rust["executable"])
        self.assertEqual(check["probe"]["command"][0], rust["executable"])

    def test_presence_only_and_display_skipped_probes_are_explicit(self):
        os.environ.pop("DISPLAY")
        report = doctor.collect_report("core")
        for check_id, executable in (("tool.xvfb", "xvfb-run"), ("native.glx", "glxinfo")):
            check = self.check(report, check_id)
            self.assertEqual(check["status"], "unverified")
            self.assertFalse(check["required"])
            self.assertEqual(check["executable"], os.path.abspath(f"/mock/bin/{executable}"))
            self.assertIsNone(check["probe"])
            self.assertIsNone(check["version"])

    def test_macos_sdk_missing_tool_and_success_record_provenance(self):
        with patch.object(doctor.sys, "platform", "darwin"):
            self.available.remove("xcrun")
            check = self.check(doctor.collect_report("native"), "native.sdk")
            self.assertEqual(check["status"], "missing")
            self.assertEqual(check["probe"]["failure"], "not_found")
            self.assertFalse(check["probe"]["attempted"])
            self.available.add("xcrun")
            self.outputs[("xcrun", "--no-cache", "--sdk", "macosx", "--show-sdk-path")] = str(self.root)
            check = self.check(doctor.collect_report("native"), "native.sdk")
            self.assertEqual(check["status"], "missing")  # Frameworks absent, query usable.
            self.assertEqual(check["executable"], os.path.abspath("/mock/bin/xcrun"))
            self.assertIsNone(check["probe"]["failure"])
            self.assertEqual(check["probe"]["returncode"], 0)

    def test_windows_presence_is_unverified_and_optional_for_web(self):
        self.available.add("link.exe")
        with patch.object(doctor.sys, "platform", "win32"):
            report = doctor.collect_report("web")
        self.assertTrue(report["ok"])
        check = self.check(report, "native.linker")
        self.assertEqual(check["status"], "unverified")
        self.assertEqual(check["executable"], os.path.abspath("/mock/bin/link.exe"))
        self.assertIsNone(check["probe"])
        self.assertIsNone(check["version"])

    def test_human_report_includes_provenance_and_failure_kind(self):
        self.outputs[("node", "--version")] = subprocess.TimeoutExpired([], 1)
        output = doctor.render_human(doctor.collect_report("web"))
        self.assertIn("Executable: " + os.path.abspath("/mock/bin/node"), output)
        self.assertIn("Version: 1.88.0", output)
        self.assertIn("Probe failure: timeout", output)
        self.assertIn("Doctor interpreter:", output)


class BoundedProcessTests(unittest.TestCase):
    def query(self, source, timeout=2):
        return doctor._probe([sys.executable, "-B", "-c", source], timeout)

    def test_real_query_decodes_invalid_utf8_and_retains_streams(self):
        status, output, evidence = self.query(
            "import os; os.write(1, b'hello\\xff'); os.write(2, b'warning\\n')")
        self.assertEqual(status, "ok")
        self.assertEqual(output, "hello\ufffd")
        self.assertEqual(evidence["executable"], os.path.abspath(sys.executable))
        self.assertEqual(evidence["probe"]["stderr"], "warning")

    def test_real_nonzero_exit_and_timeout_preserve_partial_diagnostics(self):
        for source, failure, returncode in (
            ("import sys; print('reason', file=sys.stderr); sys.exit(19)", "nonzero_exit", 19),
            ("import os, time; os.write(2, b'reason'); time.sleep(30)", "timeout", None),
        ):
            with self.subTest(failure=failure):
                started = time.monotonic()
                status, detail, evidence = self.query(source, timeout=0.5)
                self.assertEqual(status, "error")
                self.assertEqual(evidence["probe"]["failure"], failure)
                self.assertEqual(evidence["probe"]["returncode"], returncode)
                self.assertIn("reason", detail)
                self.assertLess(time.monotonic() - started, 3)

    def test_output_budget_combines_streams_and_stops_a_noisy_query(self):
        started = time.monotonic()
        with patch.object(doctor, "PROBE_OUTPUT_LIMIT", 4096):
            status, detail, evidence = self.query(
                "import os, time; os.write(1, b'o' * 3000); os.write(2, b'e' * 3000); time.sleep(30)")
        self.assertEqual(status, "error")
        self.assertEqual(evidence["probe"]["failure"], "output_limit")
        self.assertEqual(evidence["probe"]["output_limit_bytes"], 4096)
        self.assertIn("4096", detail)
        self.assertLess(time.monotonic() - started, 3)
        self.assertLessEqual(len(evidence["probe"]["stdout"]), doctor.SUMMARY_LIMIT)
        self.assertLessEqual(len(evidence["probe"]["stderr"]), doctor.SUMMARY_LIMIT)

    def test_exact_output_budget_is_allowed(self):
        status, output, evidence = self.query(f"import os; os.write(1, b'x' * {doctor.PROBE_OUTPUT_LIMIT})")
        self.assertEqual(status, "ok")
        self.assertEqual(len(output), doctor.PROBE_OUTPUT_LIMIT)
        self.assertEqual(evidence["probe"]["returncode"], 0)

    def test_large_finite_timeout_does_not_overflow_platform_waits(self):
        status, output, _ = self.query("print('ok')", timeout=sys.float_info.max)
        self.assertEqual(status, "ok")
        self.assertEqual(output, "ok")

    def test_closed_pipes_do_not_allow_a_hung_process_to_escape_timeout(self):
        status, _, evidence = self.query(
            "import os, time; os.close(1); os.close(2); time.sleep(30)", timeout=0.2)
        self.assertEqual(status, "error")
        self.assertEqual(evidence["probe"]["failure"], "timeout")

    @unittest.skipUnless(os.name == "posix", "POSIX executable and process-group semantics")
    def test_inherited_pipe_does_not_hang_after_query_process_exits(self):
        started = time.monotonic()
        status, _, evidence = self.query(
            "import subprocess, sys; subprocess.Popen([sys.executable, '-B', '-c', 'import time; time.sleep(30)'])",
            timeout=0.3,
        )
        self.assertEqual(status, "error")
        self.assertEqual(evidence["probe"]["failure"], "timeout")
        self.assertLess(time.monotonic() - started, 3)

    @unittest.skipUnless(os.name == "posix", "POSIX script interpreter semantics")
    def test_found_script_with_missing_interpreter_is_unusable(self):
        with tempfile.TemporaryDirectory(prefix="doctor broken tool ") as temporary:
            script = Path(temporary) / "tool"
            script.write_text("#!/nonexistent/ferravex-doctor-interpreter\n", encoding="utf-8")
            script.chmod(0o755)
            status, _, evidence = doctor._probe([str(script), "--version"], 1)
        self.assertEqual(status, "error")
        self.assertEqual(evidence["executable"], str(script))
        self.assertEqual(evidence["probe"]["failure"], "execution_error")

    @unittest.skipUnless(os.name == "posix", "Symlinks may require privileges elsewhere")
    def test_symlink_dispatch_name_and_spaced_path_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix="doctor shim ") as temporary:
            # Canonicalize the parent, preserving the rustc dispatch symlink.
            shim = Path(temporary).resolve() / "rustc"
            shim.symlink_to(sys.executable)
            status, output, evidence = doctor._probe(
                [str(shim), "-B", "-c", "import sys; print(sys.executable)"], 2)
        self.assertEqual(status, "ok")
        self.assertEqual(output, str(shim))
        self.assertEqual(evidence["executable"], str(shim))
        self.assertEqual(evidence["probe"]["command"][0], str(shim))

    def test_relative_lookup_becomes_absolute_without_changing_invocation_name(self):
        with patch.object(doctor.shutil, "which", return_value="relative/bin/rustc"):
            self.assertEqual(doctor._executable("rustc"), os.path.abspath("relative/bin/rustc"))


if __name__ == "__main__":
    unittest.main()
