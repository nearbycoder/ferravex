#!/usr/bin/env python3
"""Focused trusted-guide runner regressions (stdlib; no repository Cargo build).

Artifact/build cases use isolated Cargo JSON fixtures. Real process tests use
Python and, when available, rustc with std-only programs in temporary directories.
"""

from contextlib import redirect_stdout
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("check_guide_examples", Path(__file__).with_name("check-guide-examples.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class GuideTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ferravex trusted guides ")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / "repository"
        self.root.mkdir()
        self.output = self.base / "result"
        self.host = "x86_64-unknown-linux-gnu"
        self.target_dir = self.base / "custom cargo target"
        self.native = self.target_dir / self.host / "debug"
        self.deps = self.native / "deps"
        self.deps.mkdir(parents=True)
        target = dict(name="ferravex_core", kind=["lib"], crate_types=["lib"],
                      src_path=str(self.root / "crates/ferravex-core/src/lib.rs"))
        package = dict(name="ferravex-core", id="path+file:///fixture#ferravex-core@0.1.0",
                       source=None, manifest_path=str(self.root / "crates/ferravex-core/Cargo.toml"),
                       targets=[target])
        self.metadata = dict(packages=[package], workspace_members=[package["id"]],
                             target_directory=str(self.target_dir))
        self.library = self.native / "libferravex_core.rlib"
        self.library.write_bytes(b"fixture rlib")
        self.record = dict(reason="compiler-artifact", package_id=package["id"], target=target,
                           profile={"test": False}, filenames=[str(self.library)], fresh=True)
        self.finished = dict(reason="build-finished", success=True)

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def fence(self, body="fn main() {}", marker="rust,check,crate-type=bin"):
        return f"```{marker}\n{body}\n```\n"

    def invoke(self, *args, paths=("guide.md",)):
        output = io.StringIO()
        with redirect_stdout(output):
            status = runner.main(["--root", str(self.root), "--output", str(self.output),
                                  "--json", *args, *paths])
        report = json.loads(output.getvalue())
        self.assertEqual(status, report["exit_code"])
        return status, report

    def select(self, records=None):
        return runner.select_artifacts(records if records is not None else [self.record, self.finished],
                                       ["ferravex-core"], self.metadata, self.root,
                                       self.target_dir, self.host)

    def fake_command(self, argv, *, cwd, log_prefix, timeout, env=None):
        stage = log_prefix.name
        stdout = ""
        if stage == "rustc-version":
            stdout = "rustc fixture\nhost: " + self.host + "\n"
        elif stage == "metadata":
            stdout = json.dumps(self.metadata)
        elif stage == "build":
            stdout = "\n".join(json.dumps(row) for row in [self.record, self.finished])
        for stream, data in (("stdout", stdout), ("stderr", "")):
            Path(str(log_prefix) + "." + stream).write_text(data, encoding="utf-8")
        return dict(argv=list(argv), cwd=str(cwd), env=env or {}, timeout_seconds=timeout,
                    status="passed", returncode=0, stdout=str(log_prefix) + ".stdout",
                    stderr=str(log_prefix) + ".stderr", stdout_truncated=False, stderr_truncated=False)

    def test_top_level_artifact_uses_matching_host_deps_without_globbing(self):
        # A stale file and another target must not supply an extern.
        (self.deps / "libferravex_core-stale.rlib").write_bytes(b"old")
        wrong = copy.deepcopy(self.record)
        wrong["filenames"] = [str(self.target_dir / "wasm32-unknown-unknown/debug/libferravex_core.rlib")]
        artifacts = self.select([wrong, self.record, self.finished])
        core = artifacts["ferravex_core"]
        self.assertEqual(core["path"], str(self.library))
        self.assertEqual(core["dependency_dir"], str(self.deps))
        self.assertTrue(core["evidence"]["fresh"])

    def test_hashed_artifact_is_selected_only_when_cargo_reports_it(self):
        hashed = self.deps / "libferravex_core-fresh.rlib"
        hashed.write_bytes(b"fresh")
        self.record["filenames"] = [str(hashed), str(hashed.with_suffix(".rmeta"))]
        self.assertEqual(self.select()["ferravex_core"]["path"], str(hashed))

    def test_missing_ambiguous_wrong_target_and_missing_build_completion_fail(self):
        other = self.deps / "libferravex_core-second.rlib"
        other.write_bytes(b"second")
        duplicate = copy.deepcopy(self.record)
        duplicate["filenames"] = [str(other)]
        wrong = copy.deepcopy(self.record)
        wrong["filenames"] = [str(self.target_dir / "debug/libferravex_core.rlib")]
        for records in ([], [self.finished], [self.record],
                        [self.record, duplicate, self.finished], [wrong, self.finished]):
            with self.subTest(records=records), self.assertRaises(ValueError):
                self.select(records)

    def test_other_package_test_proc_macro_and_source_target_are_rejected(self):
        changes = [dict(package_id="registry+untrusted#ferravex-core@0.1.0"),
                   dict(profile={"test": True}),
                   dict(target=self.record["target"] | {"kind": ["proc-macro"]}),
                   dict(target=self.record["target"] | {"src_path": "/other/lib.rs"})]
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.select([self.record | change, self.finished])
        self.metadata["packages"][0]["source"] = "registry+untrusted"
        with self.assertRaisesRegex(ValueError, "first-party"):
            self.select()

    def test_metadata_package_must_be_expected_workspace_member(self):
        self.metadata["workspace_members"] = []
        with self.assertRaisesRegex(ValueError, "first-party"):
            self.select()

    def test_artifact_must_exist_and_symlink_must_stay_in_host_directory(self):
        self.library.unlink()
        with self.assertRaisesRegex(ValueError, "missing"):
            self.select()
        elsewhere = self.base / "outside.rlib"
        elsewhere.write_bytes(b"outside")
        self.library.symlink_to(elsewhere)
        with self.assertRaisesRegex(ValueError, "found 0"):
            self.select()

    def test_explicit_host_and_metadata_target_dir_override_cross_build_environment(self):
        self.write("guide.md", self.fence())
        with patch.object(runner, "run_command", side_effect=self.fake_command) as calls, \
                patch.object(runner.shutil, "which", side_effect=lambda name: "/tools/" + name), \
                patch.dict(os.environ, {"CARGO_BUILD_TARGET": "wasm32-unknown-unknown",
                                        "CARGO_TARGET_DIR": str(self.target_dir)}):
            status, report = self.invoke("-p", "ferravex-core")
        self.assertEqual(status, 0, report["diagnostics"])
        build = report["commands"]["build"]["argv"]
        self.assertEqual(build[build.index("--target") + 1], self.host)
        self.assertEqual(build[build.index("--target-dir") + 1], str(self.target_dir))
        self.assertEqual(build[build.index("--jobs") + 1], "2")
        self.assertIn("--offline", build)
        self.assertIn("--locked", build)
        self.assertIn("--lib", build)
        self.assertNotIn("--workspace", build)
        self.assertEqual(report["commands"]["build"]["env"], {"RUSTC": os.path.abspath("/tools/rustc")})
        compiled = report["examples"][0]["compile"]["argv"]
        self.assertEqual(compiled[compiled.index("--target") + 1], self.host)
        self.assertIn("dependency=" + str(self.deps), compiled)
        self.assertNotIn("--emit=metadata", compiled)
        self.assertEqual(len(calls.call_args_list), 6)
        self.assertEqual(json.loads((self.output / "report.json").read_text()), report)

    def test_explicit_target_directory_takes_precedence_over_metadata(self):
        self.write("guide.md", self.fence())
        self.metadata["target_directory"] = str(self.base / "unused")
        with patch.object(runner, "run_command", side_effect=self.fake_command), \
                patch.object(runner.shutil, "which", side_effect=lambda name: "/tools/" + name):
            status, report = self.invoke("-p", "ferravex-core", "--target-dir", str(self.target_dir))
        self.assertEqual(status, 0, report["diagnostics"])
        self.assertEqual(report["target_dir"], str(self.target_dir))

    def test_scope_requires_explicit_files_inside_root(self):
        self.write("guide.md", self.fence())
        outside = self.base / "outside.md"
        outside.write_text(self.fence())
        (self.root / "escape.md").symlink_to(outside)
        for scope in (".", "missing.md", "escape.md", str(outside)):
            self.output = self.base / ("result-" + str(len(list(self.base.iterdir()))))
            with self.subTest(scope=scope), patch.object(runner, "run_command") as command:
                status, report = self.invoke(paths=(scope,))
                self.assertEqual(status, 1)
                self.assertEqual(report["diagnostics"][0]["code"], "invalid-scope")
                command.assert_not_called()

    def test_links_do_not_expand_execution_scope_and_only_explicit_bins_are_extracted(self):
        selected = self.fence("# fn main() { assert_eq!(2 + 2, 4); }", "rust check crate-type=bin edition=2021")
        ignored = "".join(self.fence(marker=marker) for marker in
                          ("rust", "sh", "rust,no_run", "rust,ignore", "rust,compile_fail", "rust,check"))
        self.write("guide.md", "[linked](linked.md#heading)\n" + ignored + selected)
        self.write("linked.md", "# Heading\n" + self.fence('fn main() { panic!("not selected"); }'))
        with patch.object(runner, "run_command", side_effect=self.fake_command), \
                patch.object(runner.shutil, "which", return_value="/tools/rustc"):
            status, report = self.invoke()
        self.assertEqual(status, 0)
        self.assertEqual(report["counts"], dict(selected=1, passed=1, compiled=1, executed=1, skipped_libraries=1))
        manifest = json.loads((self.output / "examples/manifest.json").read_text())
        self.assertEqual(len(manifest["examples"]), 1)
        entry = manifest["examples"][0]
        self.assertIn("--emit=metadata", entry["argv"])
        self.assertEqual(entry["edition"], "2021")
        source = (self.output / "examples" / entry["file"]).read_bytes()
        self.assertEqual(hashlib.sha256(source).hexdigest(), entry["sha256"])
        self.assertTrue(source.startswith(b"fn main"))

    def test_link_marker_and_empty_selection_fail_before_commands(self):
        contents = ["[bad](missing.md)\n" + self.fence(), self.fence(marker="rust,check,crate-type=bin,no_run"),
                    self.fence(marker="rust,check"), "ordinary prose\n", "```rust,check,crate-type=bin\nfn main() {}"]
        for index, text in enumerate(contents):
            self.write("guide.md", text)
            self.output = self.base / f"result-{index}"
            with self.subTest(text=text), patch.object(runner, "run_command") as command:
                status, report = self.invoke()
                self.assertEqual(status, 1)
                self.assertTrue(report["diagnostics"])
                command.assert_not_called()

    def test_usage_rejects_absent_scope_invalid_packages_timeouts_and_existing_output(self):
        for args in (("--run-timeout", "nan"), ("--run-timeout", "61"), ("--compile-timeout", "0"),
                     ("--build-timeout", "inf"), ("--package", "external"), ("--jobs", "8")):
            with self.subTest(args=args), patch.object(runner, "run_command") as command:
                status, _ = self.invoke(*args)
                self.assertEqual(status, 2)
                command.assert_not_called()
        self.assertEqual(self.invoke(paths=())[0], 2)
        self.output.mkdir()
        sentinel = self.output / "existing.txt"
        sentinel.write_text("retain")
        self.assertEqual(self.invoke()[0], 2)
        self.assertEqual(sentinel.read_text(), "retain")
        self.assertFalse((self.output / "report.json").exists())

    def test_help_documents_trusted_execution_and_compile_only_checker(self):
        stream = io.StringIO()
        with redirect_stdout(stream), self.assertRaises(SystemExit) as exited:
            runner.main(["--help"])
        self.assertEqual(exited.exception.code, 0)
        self.assertIn("not a Rust sandbox", stream.getvalue())
        self.assertIn("compile-only", stream.getvalue())

    def test_build_failure_and_invalid_artifacts_never_compile_or_execute(self):
        self.write("guide.md", self.fence())
        for mode in ("failed", "timeout", "malformed", "no-artifacts", "truncated"):
            self.output = self.base / ("result-" + mode)

            def fail_build(argv, **kwargs):
                result = self.fake_command(argv, **kwargs)
                if kwargs["log_prefix"].name == "build":
                    if mode in {"failed", "timeout"}:
                        result.update(status=mode, returncode=17)
                    elif mode == "truncated":
                        result["stdout_truncated"] = True
                    else:
                        Path(result["stdout"]).write_text("malformed" if mode == "malformed" else json.dumps(self.finished))
                return result

            with self.subTest(mode=mode), patch.object(runner, "run_command", side_effect=fail_build), \
                    patch.object(runner.shutil, "which", side_effect=lambda name: "/tools/" + name):
                status, report = self.invoke("-p", "ferravex-core")
                self.assertEqual(status, 1)
                self.assertIsNone(report["examples"][0]["compile"])
                self.assertIsNone(report["examples"][0]["run"])
                self.assertEqual(report["counts"]["executed"], 0)

    def test_missing_compiler_is_structured_and_does_not_count_execution(self):
        self.write("guide.md", self.fence())
        with patch.object(runner.shutil, "which", return_value=None):
            status, report = self.invoke()
        self.assertEqual(status, 1)
        self.assertEqual(report["diagnostics"][0]["code"], "missing-rustc")
        self.assertEqual(report["counts"]["executed"], 0)

    def test_compile_timeout_is_separate_from_not_executed(self):
        self.write("guide.md", self.fence())

        def timeout_compile(argv, **kwargs):
            result = self.fake_command(argv, **kwargs)
            if kwargs["log_prefix"].name.endswith("-compile"):
                result.update(status="timeout", returncode=-9)
            return result

        with patch.object(runner, "run_command", side_effect=timeout_compile), \
                patch.object(runner.shutil, "which", return_value="/tools/rustc"):
            status, report = self.invoke()
        self.assertEqual(status, 1)
        self.assertIsNone(report["examples"][0]["run"])
        self.assertEqual(report["examples"][0]["compile"]["status"], "timeout")
        self.assertEqual(report["counts"]["executed"], 0)

    def test_command_failure_timeout_missing_tool_and_bounded_logs(self):
        cases = [("failure", "import sys; print('failed', file=sys.stderr); sys.exit(7)", 5, "failed"),
                 # The deadline includes interpreter startup, which can take
                 # more than 200 ms on a cold/loaded macOS CI runner.
                 ("timeout", "import time; print('started', flush=True); time.sleep(20)", 5, "timeout"),
                 ("large", "print('x' * 100)", 5, "passed")]
        with patch.object(runner, "MAX_LOG_BYTES", 16):
            for name, body, timeout, expected in cases:
                result = runner.run_command([sys.executable, "-c", body], cwd=self.root,
                                            log_prefix=self.base / name, timeout=timeout)
                self.assertEqual(result["status"], expected)
                if name == "failure":
                    self.assertEqual(result["returncode"], 7)
                    self.assertIn("failed", runner.read_log(result, "stderr"))
                if name == "timeout":
                    self.assertIn("started", runner.read_log(result))
                if name == "large":
                    self.assertTrue(result["stdout_truncated"])
                    self.assertEqual(Path(result["stdout"]).stat().st_size, 16)
        result = runner.run_command([str(self.base / "missing tool")], cwd=self.root,
                                    log_prefix=self.base / "missing", timeout=1)
        self.assertEqual(result["status"], "spawn_failed")
        self.assertIsNone(result["returncode"])

    @unittest.skipUnless(os.name == "posix", "process-group cleanup is POSIX-specific")
    def test_timeout_kills_descendant_before_it_writes_after_parent_exit(self):
        sentinel = self.base / "child-finished"
        child = "import time; from pathlib import Path; time.sleep(1); Path(" + repr(str(sentinel)) + ").touch()"
        parent = "import subprocess, sys, time; subprocess.Popen([sys.executable, '-c', " + repr(child) + "]); time.sleep(20)"
        result = runner.run_command([sys.executable, "-c", parent], cwd=self.root,
                                    log_prefix=self.base / "descendant", timeout=.3)
        self.assertEqual(result["status"], "timeout")
        time.sleep(1.1)
        self.assertFalse(sentinel.exists())

    @unittest.skipUnless(shutil.which("rustc"), "real std-only checks require rustc")
    def test_real_compile_failure_assertion_failure_and_success_are_independent(self):
        text = (self.fence("# fn main() {\n#     let _: u8 = false;\n# }")
                + self.fence('fn main() { panic!("deliberate runtime failure"); }')
                + self.fence('fn main() { assert_eq!(2 + 2, 4); println!("passed"); }'))
        self.write("guide.md", text)
        status, report = self.invoke()
        self.assertEqual(status, 1)
        self.assertEqual([row["status"] for row in report["examples"]],
                         ["compile_failed", "run_failed", "passed"])
        self.assertEqual(report["counts"]["compiled"], 2)
        self.assertEqual(report["counts"]["executed"], 2)
        self.assertEqual(report["counts"]["passed"], 1)
        self.assertIsNone(report["examples"][0]["run"])
        diagnostic = next(row for row in report["diagnostics"] if row["code"] == "rust-compile")
        self.assertEqual((diagnostic["path"], diagnostic["line"]), ("guide.md", 3))
        self.assertGreater(diagnostic["column"], 2)  # Hidden '# ' is restored.
        self.assertIn("deliberate runtime failure", runner.read_log(report["examples"][1]["run"], "stderr"))
        self.assertEqual(runner.read_log(report["examples"][2]["run"]), "passed\n")

    @unittest.skipUnless(shutil.which("rustc"), "real std-only checks require rustc")
    def test_real_binary_timeout_and_success_without_cargo(self):
        self.write("guide.md", self.fence("fn main() { loop { std::thread::park(); } }") + self.fence())
        status, report = self.invoke("--run-timeout", "0.2")
        self.assertEqual(status, 1)
        self.assertEqual(report["examples"][0]["run"]["status"], "timeout")
        self.assertEqual(report["examples"][1]["status"], "passed")
        self.assertNotIn("build", report["commands"])


if __name__ == "__main__":
    unittest.main()
