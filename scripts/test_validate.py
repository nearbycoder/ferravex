#!/usr/bin/env python3
"""Validator regressions: no builds, displays, subprocess suites or network needed."""

from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

import validate


class PlanTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root / "scripts").mkdir()
        (self.root / "web/tests").mkdir(parents=True)
        for path in ("scripts/test_future.py", "web/tests/catalog.mjs", "web/tests/storage.mjs",
                     "web/tests/wasm-future.mjs"):
            (self.root / path).touch()

    def plan(self, **kwargs):
        return validate.build_plan(self.root, **kwargs)

    def test_fast_selects_source_suites_and_new_cli_tools(self):
        plan = self.plan()
        ids = {check.id for check in plan}
        self.assertTrue({"rust.test", "python.catalog", "python.test_ferravex_cli",
                         "python.test_doctor", "python.test_new_game", "python.test_catalog",
                         "python.test_validate", "python.test_future", "js.catalog",
                         "js.loader", "js.lifecycle", "js.platform-input"} <= ids)
        self.assertTrue(all(check.group in validate.FAST_GROUPS for check in plan))
        self.assertFalse(any(check.full_only for check in plan))
        for name in ("wasm.details", "wasm.gpu-timer", "wasm.wasm-future", "wasm.templates", "rust.clippy", "rust.guide-examples", "site.test_site"):
            self.assertNotIn(name, ids)

    def test_full_orders_and_deduplicates_all_prerequisites(self):
        plan = self.plan(mode="full")
        ids = [check.id for check in plan]
        self.assertEqual(len(ids), len(set(ids)))
        for position, check in enumerate(plan):
            self.assertTrue(set(check.requires) <= set(ids[:position]), check.id)
        self.assertTrue({"rust.clippy", "rust.guide-examples", "wasm.details", "wasm.gpu-timer", "wasm.wasm-future",
                         "wasm.multiplayer-native", "wasm.templates", "native.headless", "native.smoke", "native.gpu",
                         "site.test_site", "site.test_search", "site.test_player_release"} <= set(ids))
        native = next(check for check in plan if check.id == "native.smoke")
        self.assertIn("linux-display", native.needs)
        headless = next(check for check in plan if check.id == "native.headless")
        self.assertNotIn("linux-display", headless.needs)
        self.assertEqual(headless.command.count("--game"), 2)

    def test_dual_host_suites_never_read_staged_artifacts_in_fast_mode(self):
        fast = {check.id: check for check in self.plan(mode="fast", groups=("js",))}
        for name in ("details", "visuals"):
            self.assertEqual(fast["js." + name].command,
                             ("node", f"web/tests/{name}.mjs", "--source-only"))
            self.assertFalse(fast["js." + name].requires)
            self.assertNotIn("wasm." + name, fast)
            changed = {check.id: check for check in self.plan(
                mode="changed", changed_paths=[f"web/tests/{name}.mjs"])}
            self.assertIn("js." + name, changed)
            self.assertEqual(changed["wasm." + name].requires, ("wasm.build",))
            self.assertNotIn("--source-only", changed["wasm." + name].command)

    def test_search_fixtures_and_shipped_module_have_separate_prerequisites(self):
        fast = {check.id: check for check in self.plan(mode="fast", groups=("python",))}
        source = fast["python.search-source"]
        self.assertEqual(source.command[-2:], ("CorpusTests", "BrowserSearchTests"))
        self.assertFalse(source.requires)
        self.assertIn("node", source.needs)
        self.assertNotIn("site.search-browser-module", fast)
        full = self.plan(mode="full", groups=("site",))
        ids = [check.id for check in full]
        shipped = next(check for check in full if check.id == "site.search-browser-module")
        self.assertEqual(shipped.requires, ("site.build",))
        self.assertLess(ids.index("wasm.build"), ids.index("site.build"))
        self.assertLess(ids.index("site.build"), ids.index(shipped.id))

    def test_group_selection_includes_only_required_other_group_checks(self):
        ids = {check.id for check in self.plan(mode="full", groups=("site", "site"))}
        self.assertIn("wasm.build", ids)
        self.assertIn("rust.owned", ids)
        self.assertNotIn("wasm.multiplayer-native", ids)
        self.assertNotIn("rust.test", ids)
        self.assertNotIn("native.smoke", ids)
        python = self.plan(mode="full", groups=("python",))
        self.assertTrue(all(check.group == "python" for check in python))

    def test_catalog_reports_migration_in_fast_and_requires_it_in_full(self):
        for mode, flag in (("fast", "--migration"), ("full", "--require-session")):
            audit = next(check for check in self.plan(mode=mode) if check.id == "python.catalog")
            self.assertIn(flag, audit.command)
            self.assertIn("--json", audit.command)

    def test_cargo_checks_are_offline_locked_bounded_and_format_is_read_only(self):
        for check in self.plan(mode="full", jobs=1):
            if check.command[0] != "cargo":
                continue
            if check.id == "rust.fmt":
                self.assertEqual(check.command[-2:], ("--", "--check"))
            else:
                self.assertIn("--offline", check.command, check.id)
                self.assertIn("--locked", check.command, check.id)
                self.assertEqual(check.command[check.command.index("--jobs") + 1], "1")
        tests = next(check for check in self.plan() if check.id == "rust.test")
        self.assertNotIn("--all-targets", tests.command, "cargo test must retain its default rustdoc coverage")

    def test_rejects_incoherent_or_unbounded_options(self):
        for options in ({"mode": "typo"}, {"groups": ("typo",)}, {"groups": ("wasm",)},
                        {"jobs": 0}, {"jobs": 3}, {"frames": 0}, {"mode": "changed"},
                        {"changed_paths": ["web/loader.js"]},
                        {"mode": "full", "changed_paths": ["docs/INPUT.md"]}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                self.plan(**options)

    def test_changed_documentation_includes_build_prerequisites_once(self):
        plan = self.plan(mode="changed", changed_paths=["docs/INPUT.md", "./docs/INPUT.md"])
        ids = [check.id for check in plan]
        self.assertEqual(len(ids), len(set(ids)))
        for position, check in enumerate(plan):
            self.assertTrue(set(check.requires) <= set(ids[:position]), check.id)
        self.assertTrue({"wasm.build", "site.docs", "site.build", "rust.owned", "python.catalog"} <= set(ids))
        self.assertNotIn("native.build", ids)
        self.assertNotIn("rust.test", ids)
        self.assertNotIn("wasm.multiplayer-native", ids)
        audit = next(check for check in plan if check.id == "python.catalog")
        self.assertIn("--require-session", audit.command)

    def test_changed_groups_add_coverage_without_dropping_affected_checks(self):
        changed = self.plan(mode="changed", changed_paths=["scripts/doctor.py"], groups=("js", "js"))
        expected = self.plan(mode="full", groups=("python", "js"))
        self.assertEqual(changed, expected)
        self.assertEqual(self.plan(mode="changed", changed_paths=[]), [])
        self.assertEqual(self.plan(mode="changed", changed_paths=[], groups=("python",)),
                         self.plan(mode="full", groups=("python",)))

    def test_unknown_and_shared_inputs_conservatively_select_full_checks(self):
        for path in ("future/subsystem.dat", "Cargo.lock", "crates/ferravex-core/src/lib.rs",
                     "examples/emberline/Cargo.toml", "examples/catalog.json", "scripts/validate.py",
                     "scripts/test_validate.py", ".github/workflows/ci.yml", ".cargo/config.toml"):
            with self.subTest(path=path):
                self.assertEqual(self.plan(mode="changed", changed_paths=[path]), self.plan(mode="full"))

    def test_changed_classification_is_independent_of_files_and_artifacts(self):
        paths = ["docs/deleted file.md", "web/tests/wasm-future.mjs"]
        before = self.plan(mode="changed", changed_paths=paths)
        (self.root / "web/tests/wasm-future.mjs").unlink()
        # Full discovery may lose an optional suite, but required build checks
        # and group selection cannot disappear because an old output exists.
        for path in ("dist/web/future.wasm", "target/validation/results.json"):
            output = self.root / path
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text("stale pass")
        after = self.plan(mode="changed", changed_paths=paths)
        self.assertEqual({check.group for check in before}, {check.group for check in after})
        self.assertIn("wasm.build", {check.id for check in after})


class ChangedPathTests(unittest.TestCase):
    def test_rules_explain_group_selection(self):
        cases = {
            "docs/INPUT.md": ("python", "site"),
            "README.md": ("python", "site"),
            "website/static/app.js": ("python", "site"),
            "templates/game-2d/src/lib.rs.in": ("python",),
            "scripts/doctor.py": ("python",),
            "scripts/test_future.py": ("python",),
            "scripts/test_site.py": ("python", "site"),
            "scripts/build-site.py": ("python", "site"),
            "scripts/serve-site.py": ("python", "site"),
            "scripts/serve-web.py": ("python",),
            "web/loader.js": ("python", "js", "wasm", "site"),
            "web/tests/loader.mjs": ("js",),
            "web/tests/details.mjs": ("js", "wasm"),
            "web/tests/visuals.mjs": ("js", "wasm"),
            "web/tests/wasm-future.mjs": ("wasm",),
        }
        for path, groups in cases.items():
            with self.subTest(path=path):
                impact, = validate.changed_path_groups([path])
                self.assertEqual(impact["groups"], list(groups))
                self.assertTrue(impact["reason"])
                self.assertFalse(impact["fallback"])
        for path in ("future/unknown.file", "scripts/new-build-tool.py", "target/old-pass.json"):
            impact, = validate.changed_path_groups([path])
            self.assertEqual(impact["groups"], list(validate.GROUPS))
            self.assertTrue(impact["fallback"])

    def test_relative_normalization_keeps_deleted_and_unusual_names(self):
        paths = ["./docs//deleted file.md", "docs/deleted file.md", "docs/new\nname.md", "web/-host.js"]
        self.assertEqual([row["path"] for row in validate.changed_path_groups(paths)],
                         ["docs/deleted file.md", "docs/new\nname.md", "web/-host.js"])

    def test_bad_paths_cannot_silently_become_narrow_scope(self):
        for path in ("", ".", "./", "../docs/README.md", "docs/../Cargo.toml", "/docs/README.md",
                     "C:/docs/README.md", "C:README.md", "docs\\README.md", "a\0b", None, 3):
            with self.subTest(path=path), self.assertRaises(ValueError):
                validate.changed_path_groups([path])
        with self.assertRaises(ValueError):
            validate.changed_path_groups("docs/README.md")

    def test_nul_file_preserves_both_rename_sides_and_empty_means_no_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "changed.z"
            path.write_bytes(b"docs/old name.md\0web/new\nname.js\0docs/old name.md\0")
            self.assertEqual(validate.read_changed_paths(path), ["docs/old name.md", "web/new\nname.js"])
            path.write_bytes(b"")
            self.assertEqual(validate.read_changed_paths(path), [])
            for content in (b"docs/README.md\n", b"\0", b"docs/README.md\0\0", b"../README.md\0"):
                path.write_bytes(content)
                with self.subTest(content=content), self.assertRaises(ValueError):
                    validate.read_changed_paths(path)


class ExecutionTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.probes = Mock()
        self.probes.missing.return_value = None
        self.run = Mock(return_value=subprocess.CompletedProcess(["fake"], 0, "hello\n", ""))

    def execute(self, checks, **kwargs):
        return validate.run_checks(checks, root=self.root, env={}, prerequisites=self.probes,
                                   run=self.run, **kwargs)

    def check(self, id="test", **kwargs):
        return validate.Check(id, "python", ("fake", "a path with spaces"), **kwargs)

    def test_fresh_evidence_directory_is_unique_and_dry_runs_do_not_allocate(self):
        check = validate.Check("examples", "rust", ("fake", "--output", "{fresh_output}/results",
                              "--target-dir", "{fresh_output}/cargo"), fresh_output=True)
        dry = self.execute([check], dry_run=True)[0]
        self.assertNotIn("artifact_directory", dry)
        self.assertEqual(list(self.root.iterdir()), [])
        self.run.assert_not_called()
        first = self.execute([check])[0]
        second = self.execute([check])[0]
        self.assertNotEqual(first["artifact_directory"], second["artifact_directory"])
        for row in (first, second):
            directory = Path(row["artifact_directory"])
            self.assertTrue(directory.is_dir())
            self.assertFalse((directory / "results").exists())
            self.assertEqual(row["command"][2], str(directory / "results"))
            self.assertEqual(row["command"][4], str(directory / "cargo"))
        self.assertEqual(self.run.call_args.args[0], tuple(second["command"]))

    def test_failure_blocks_consumers_but_independent_checks_still_run(self):
        self.run.side_effect = [subprocess.CompletedProcess(["fake"], 7, "", "bad build"),
                                subprocess.CompletedProcess(["fake"], 0, "ok", "")]
        rows = self.execute([self.check("build"), self.check("consumer", requires=("build",)), self.check("independent")])
        self.assertEqual([row["status"] for row in rows], ["failed", "skipped", "passed"])
        self.assertEqual(rows[0]["exit_code"], 7)
        self.assertIn("bad build", rows[0]["output_tail"])
        self.assertIn("build", rows[1]["reason"])
        self.assertEqual(self.run.call_count, 2)
        self.assertEqual(validate.aggregate(rows)["exit_code"], 1)

    def test_rerun_executes_build_again_and_never_uses_stale_artifacts(self):
        artifact = self.root / "dist/old.wasm"
        artifact.parent.mkdir()
        artifact.write_bytes(b"old build")
        checks = [self.check("build"), self.check("consumer", requires=("build",))]
        self.assertEqual([row["status"] for row in self.execute(checks)], ["passed", "passed"])
        self.run.return_value = subprocess.CompletedProcess(["fake"], 7, "", "new build failed")
        rows = self.execute(checks)
        self.assertEqual([row["status"] for row in rows], ["failed", "skipped"])
        self.assertEqual(self.run.call_count, 3)
        self.assertTrue(artifact.exists())

    def test_missing_tool_is_skipped_but_missing_source_is_failed(self):
        self.probes.missing.return_value = "node is not on PATH"
        rows = self.execute([self.check("optional", needs=("node",)),
                             self.check("source", sources=("missing.py",), needs=("node",))])
        self.assertEqual([row["status"] for row in rows], ["skipped", "failed"])
        self.assertIn("missing repository source", rows[1]["reason"])
        self.run.assert_not_called()

    def test_dry_run_does_not_probe_launch_or_write(self):
        logs = self.root / "logs"
        rows = self.execute([self.check("a", sources=("missing",), needs=("cargo",)),
                             self.check("b", requires=("a",))], dry_run=True, log_dir=logs)
        self.assertEqual([row["status"] for row in rows], ["planned", "planned"])
        self.run.assert_not_called()
        self.probes.missing.assert_not_called()
        self.assertFalse(logs.exists())
        self.assertEqual(validate.aggregate(rows, require_complete=True)["status"], "planned")

    def test_timeout_and_launch_error_are_failures_and_other_work_continues(self):
        self.run.side_effect = [subprocess.TimeoutExpired(["fake"], 5, b"partial", b"timeout"),
                                OSError("cannot launch"), subprocess.CompletedProcess(["fake"], 0, "", "")]
        rows = self.execute([self.check("slow"), self.check("broken"), self.check("next")], timeout=5)
        self.assertEqual([row["status"] for row in rows], ["failed", "failed", "passed"])
        self.assertIn("5 seconds", rows[0]["reason"])
        self.assertEqual(rows[0]["output_tail"], "partialtimeout")
        self.assertEqual(rows[1]["reason"], "cannot launch")

    def test_commands_are_argument_vectors_and_output_goes_to_logs(self):
        rows = self.execute([self.check()], log_dir=self.root / "logs")
        args, kwargs = self.run.call_args
        self.assertEqual(args[0], ("fake", "a path with spaces"))
        self.assertNotIn("shell", kwargs)
        self.assertEqual(kwargs["cwd"], self.root)
        self.assertEqual(Path(rows[0]["log"]).read_text(), "hello\n")

    def test_skipped_smoke_results_cannot_be_reported_as_passed(self):
        path = self.root / "smoke.json"

        def run(*args, **kwargs):
            path.write_text(json.dumps({"results": [{"game": "a", "passed": True},
                                                      {"game": "b", "status": "skipped", "passed": False}]}))
            return subprocess.CompletedProcess(args[0], 0, "", "")

        self.run.side_effect = run
        rows = self.execute([self.check(smoke_report="smoke.json")])
        self.assertEqual(rows[0]["status"], "skipped")
        self.assertEqual(len(rows[0]["details"]["results"]), 2)
        self.assertEqual(validate.aggregate(rows, require_complete=True)["exit_code"], 1)

    def test_smoke_requires_fresh_nonempty_successful_report(self):
        path = self.root / "smoke.json"
        path.write_text(json.dumps({"results": [{"passed": True}]}))
        rows = self.execute([self.check(smoke_report="smoke.json")])
        self.assertEqual(rows[0]["status"], "failed", "stale report must be removed before the command")
        for data, expected in (({"results": []}, "failed"), ({"results": [{"passed": False}]}, "failed"),
                               ({"results": [{"passed": True}]}, "passed"), ([], "failed")):
            def run(*args, **kwargs):
                path.write_text(json.dumps(data))
                return subprocess.CompletedProcess(args[0], 0, "", "")

            self.run.side_effect = run
            with self.subTest(data=data):
                rows = self.execute([self.check(smoke_report="smoke.json")])
                self.assertEqual(rows[0]["status"], expected)

    def test_smoke_nonzero_keeps_per_game_details_and_distinguishes_environment(self):
        path = self.root / "smoke.json"
        cases = (
            ([{"status": "blocked", "passed": False, "environment_limitation": "no DISPLAY"}], "skipped"),
            ([{"status": "blocked", "passed": False, "error": "bad metadata"}], "failed"),
            ([{"status": "failed", "passed": False}], "failed"),
            ([{"status": "passed", "passed": True}], "failed"),
        )
        for details, expected in cases:
            def run(*args, **kwargs):
                path.write_text(json.dumps({"results": details}))
                return subprocess.CompletedProcess(args[0], 1, "", "failed or blocked")

            self.run.side_effect = run
            with self.subTest(details=details):
                rows = self.execute([self.check(smoke_report="smoke.json")])
                self.assertEqual(rows[0]["status"], expected)
                self.assertEqual(rows[0]["exit_code"], 1)
                self.assertEqual(rows[0]["details"]["results"], details)


class ReportTests(unittest.TestCase):
    def test_aggregation_never_calls_skips_passes(self):
        rows = [{"status": "passed"}, {"status": "skipped"}]
        summary = validate.aggregate(rows)
        self.assertEqual(summary["status"], "incomplete")
        self.assertEqual(summary["counts"]["passed"], 1)
        self.assertFalse(summary["complete"])
        self.assertEqual(summary["exit_code"], 0)
        self.assertEqual(validate.aggregate(rows, require_complete=True)["exit_code"], 1)
        self.assertTrue(validate.aggregate(rows[:1])["complete"])
        self.assertFalse(validate.aggregate([])["complete"])
        self.assertEqual(validate.aggregate([], require_complete=True)["exit_code"], 1)
        empty_plan = validate.aggregate([], require_complete=True, dry_run=True)
        self.assertEqual(empty_plan["status"], "planned")
        self.assertEqual(empty_plan["exit_code"], 0)
        self.assertFalse(empty_plan["complete"])
        with self.assertRaises(ValueError):
            validate.aggregate([{"status": "unknown"}])

    def test_nested_scripts_inherit_offline_and_two_job_policy(self):
        with patch.dict(os.environ, {"CARGO_NET_OFFLINE": "false", "CARGO_BUILD_JOBS": "99",
                                     "CARGO_BUILD_TARGET": "wasm32-unknown-unknown"}):
            env = validate.child_environment(2)
        self.assertEqual(env["CARGO_NET_OFFLINE"], "true")
        self.assertEqual(env["CARGO_BUILD_JOBS"], "2")
        self.assertEqual(env["PYTHONDONTWRITEBYTECODE"], "1")
        self.assertNotIn("CARGO_BUILD_TARGET", env)
        self.assertEqual(env["CARGO_TARGET_DIR"], str(validate.ROOT / "target"))

    def test_json_dry_run_is_parseable_and_never_writes_requested_report(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            stdout, stderr = io.StringIO(), io.StringIO()
            with redirect_stdout(stdout), redirect_stderr(stderr), patch.object(validate.subprocess, "run") as run:
                code = validate.main(["--mode", "full", "--group", "site", "--dry-run", "--json", "--report", str(path)])
            report = json.loads(stdout.getvalue())
            self.assertEqual(code, 0)
            self.assertEqual(report["schema_version"], 2)
            self.assertEqual(report["summary"]["status"], "planned")
            self.assertEqual(report["requested_groups"], ["site"])
            self.assertEqual(report["included_groups"], ["rust", "site", "wasm"])
            self.assertFalse(path.exists())
            run.assert_not_called()

    def test_changed_cli_explains_paths_and_omissions_without_probes_or_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            changed_file = Path(directory) / "paths.z"
            changed_file.write_bytes(b"docs/deleted.md\0docs/other.md\0")
            report_path = Path(directory) / "no-report.json"
            stdout, stderr = io.StringIO(), io.StringIO()
            with redirect_stdout(stdout), redirect_stderr(stderr), \
                    patch.object(validate.Prerequisites, "missing") as probe, \
                    patch.object(validate.subprocess, "run") as run:
                code = validate.main(["--mode", "changed", "--changed-path", "./docs/deleted.md",
                                      "--changed-paths-file", str(changed_file), "--dry-run", "--json",
                                      "--require-complete", "--report", str(report_path)])
            report = json.loads(stdout.getvalue())
            self.assertEqual(code, 0)
            self.assertEqual(report["requested_groups"], ["python", "site"])
            self.assertEqual(report["included_groups"], ["python", "rust", "site", "wasm"])
            self.assertEqual([item["path"] for item in report["scope"]["changed_paths"]],
                             ["docs/deleted.md", "docs/other.md"])
            self.assertIn("rust.test", report["scope"]["omitted_checks"])
            self.assertIn("native.smoke", report["scope"]["omitted_checks"])
            self.assertEqual(report["summary"]["status"], "planned")
            self.assertFalse(report["summary"]["complete"])
            self.assertFalse(report["summary"]["full_complete"])
            self.assertFalse(report_path.exists())
            probe.assert_not_called()
            run.assert_not_called()

    def test_only_a_fresh_full_success_can_claim_full_completion(self):
        cases = (
            (["--mode", "full"], True),
            (["--mode", "fast"], False),
            (["--mode", "full", "--group", "python"], False),
            (["--mode", "changed", "--changed-path", "scripts/doctor.py"], False),
            (["--mode", "changed", "--changed-path", "unknown/new.file"], False),
        )
        for arguments, full_complete in cases:
            with self.subTest(arguments=arguments), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "results.json"
                stdout, stderr = io.StringIO(), io.StringIO()

                def execute(checks, **kwargs):
                    return [{"id": check.id, "status": "passed"} for check in checks]

                with patch.object(validate, "run_checks", side_effect=execute), \
                        redirect_stdout(stdout), redirect_stderr(stderr):
                    code = validate.main([*arguments, "--json", "--require-complete", "--report", str(path)])
                report = json.loads(stdout.getvalue())
                self.assertEqual(code, 0)
                self.assertTrue(report["summary"]["complete"])
                self.assertEqual(report["summary"]["full_complete"], full_complete)
                self.assertEqual(report["scope"]["selected_check_count"], len(report["results"]))

    def test_empty_changed_cli_is_never_passed_even_with_a_stale_report(self):
        with tempfile.TemporaryDirectory() as directory:
            changed = Path(directory) / "empty.z"
            changed.touch()
            path = Path(directory) / "old-report.json"
            stale = '{"summary": {"complete": true, "full_complete": true}}'
            for dry_run in (True, False):
                path.write_text(stale)
                stdout, stderr = io.StringIO(), io.StringIO()
                args = ["--mode", "changed", "--changed-paths-file", str(changed),
                        "--json", "--require-complete", "--report", str(path)]
                with redirect_stdout(stdout), redirect_stderr(stderr):
                    code = validate.main(args + (["--dry-run"] if dry_run else []))
                report = json.loads(stdout.getvalue())
                self.assertEqual(code, 0 if dry_run else 1)
                self.assertEqual(report["results"], [])
                self.assertEqual(report["requested_groups"], [])
                self.assertEqual(report["summary"]["status"], "planned" if dry_run else "incomplete")
                self.assertFalse(report["summary"]["complete"])
                self.assertFalse(report["summary"]["full_complete"])
                if dry_run:
                    self.assertEqual(path.read_text(), stale)
                else:
                    self.assertEqual(json.loads(path.read_text()), report)

    def test_full_failure_skip_or_dry_run_cannot_claim_full_completion(self):
        for status in ("failed", "skipped", "planned"):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "report.json"
                path.write_text('{"summary": {"complete": true, "full_complete": true}}')
                stdout, stderr = io.StringIO(), io.StringIO()

                def execute(checks, **kwargs):
                    rows = [{"id": check.id, "status": "passed"} for check in checks]
                    rows[-1]["status"] = status
                    return rows

                with patch.object(validate, "run_checks", side_effect=execute) as run, \
                        redirect_stdout(stdout), redirect_stderr(stderr):
                    code = validate.main(["--mode", "full", "--json", "--require-complete", "--report", str(path)])
                report = json.loads(stdout.getvalue())
                self.assertEqual(code, 0 if status == "planned" else 1)
                self.assertFalse(report["summary"]["complete"])
                self.assertFalse(report["summary"]["full_complete"])
                self.assertEqual(json.loads(path.read_text()), report)
                run.assert_called_once()

    def test_text_plan_labels_selected_coverage_and_path_reason(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            code = validate.main(["--mode", "changed", "--changed-path", "web/tests/loader.mjs", "--dry-run"])
        self.assertEqual(code, 0)
        self.assertIn("PLANNED", stdout.getvalue())
        self.assertIn("full_complete=false", stdout.getvalue())
        self.assertIn('"web/tests/loader.mjs" -> js: source-only host tests', stdout.getvalue())

    def test_changed_cli_rejects_incoherent_inputs_before_execution(self):
        cases = (["--mode", "changed"], ["--changed-path", "docs/README.md"],
                 ["--mode", "full", "--changed-path", "docs/README.md"],
                 ["--mode", "changed", "--changed-path", "../outside"],
                 ["--mode", "changed", "--changed-paths-file", "/missing/changed.z"])
        for args in cases:
            with self.subTest(args=args), redirect_stderr(io.StringIO()), \
                    patch.object(validate, "run_checks") as run, self.assertRaises(SystemExit) as error:
                validate.main(args)
            self.assertEqual(error.exception.code, 2)
            run.assert_not_called()

    def test_missing_display_is_a_skip_and_probe_is_cached(self):
        probe = validate.Prerequisites({})
        with patch.object(validate.sys, "platform", "linux"), patch.object(validate.ctypes.util, "find_library") as library:
            self.assertIn("DISPLAY is unset", probe.missing("linux-display"))
            self.assertEqual(probe.missing("linux-display"), probe.missing("linux-display"))
            library.assert_not_called()
        with patch.object(validate.sys, "platform", "darwin"):
            self.assertIn("not implemented", validate.Prerequisites({"DISPLAY": ":0"}).missing("linux-display"))

    def test_execution_report_matches_stdout_and_failure_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nested/report.json"
            stdout, stderr = io.StringIO(), io.StringIO()
            rows = [{"id": "fixture", "status": "failed", "reason": "a regression"}]
            with patch.object(validate, "run_checks", return_value=rows), redirect_stdout(stdout), redirect_stderr(stderr):
                code = validate.main(["--group", "python", "--json", "--report", str(path)])
            self.assertEqual(code, 1)
            self.assertEqual(json.loads(path.read_text()), json.loads(stdout.getvalue()))
            self.assertEqual(json.loads(path.read_text())["summary"]["status"], "failed")

    def test_missing_wasm_standard_library_is_a_skip(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(validate.shutil, "which", return_value="rustc"), patch.object(validate.subprocess, "run") as run:
            run.return_value = subprocess.CompletedProcess([], 0, directory + "\n", "")
            self.assertIn("standard library is unavailable", validate.Prerequisites({}).missing("wasm-target"))
            (Path(directory) / "libstd-fixture.rlib").touch()
            self.assertIsNone(validate.Prerequisites({}).missing("wasm-target"))


if __name__ == "__main__":
    unittest.main()
