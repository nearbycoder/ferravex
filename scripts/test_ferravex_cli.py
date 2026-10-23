"""CLI regressions; only temporary Python helpers are executed, never Cargo/games."""

from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("ferravex_cli", Path(__file__).with_name("ferravex.py"))
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)


class CliTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="ferravex cli ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        (self.root / "examples").mkdir()
        (self.root / "scripts").mkdir()
        self.games = [
            {"id": "fresh-demo", "title": "Fresh demo", "host": "fresh-demo", "dimension": "2D"},
            {"id": "owned-lab", "title": "Courtyard", "host": "owned", "dimension": "3D"},
        ]
        self.write_catalog(self.games)
        for game in self.games:
            directory = self.root / "examples" / game["id"]
            directory.mkdir()
            (directory / "Cargo.toml").write_text(f'[package]\nname = "{game["id"]}"\n')
        core = self.root / "crates" / "core-directory"
        core.mkdir(parents=True)
        (core / "Cargo.toml").write_text('[package]\nname = "ferravex-core"\n', encoding="utf-8")
        self.write_workspace(["examples/*", "crates/*"])
        for name in ("build-web.py", "doctor.py", "new-game.py"):
            (self.root / "scripts" / name).write_text("# Routing fixture; never executed.\n")
        self.runner = patch.object(cli.subprocess, "run")
        self.run = self.runner.start()
        self.addCleanup(self.runner.stop)
        self.run.return_value = subprocess.CompletedProcess([], 0)

    def write_catalog(self, value):
        (self.root / "examples" / "catalog.json").write_text(json.dumps(value), encoding="utf-8")

    def write_workspace(self, members, *, exclude=(), root_package=None):
        text = '[workspace]\nmembers = ' + json.dumps(members)
        text += '\nexclude = ' + json.dumps(list(exclude)) + '\n'
        if root_package is not None:
            text += '[package]\nname = ' + json.dumps(root_package) + '\n'
        (self.root / "Cargo.toml").write_text(text, encoding="utf-8")

    def invoke(self, *args):
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            try:
                status = cli.main(list(args), root=self.root)
            except SystemExit as error:
                status = error.code
        return status, output.getvalue(), errors.getvalue()

    def plan(self, *args):
        status, output, errors = self.invoke("--dry-run", *args)
        self.assertEqual((status, errors), (0, ""))
        self.run.assert_not_called()
        return json.loads(output)

    def outcome(self, *args, expected=0):
        status, output, errors = self.invoke("--result-json", *args)
        self.assertEqual((status, errors), (expected, ""))
        report = json.loads(output)
        self.assertEqual(set(report), {
            "schema_version", "command", "status", "exit_code", "child_returncode",
            "dry_run", "plan", "data", "error",
        })
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["exit_code"], expected)
        if report["error"] is not None:
            self.assertEqual(set(report["error"]), {"code", "message", "hint", "details"})
            self.assertTrue(report["error"]["message"])
            self.assertTrue(report["error"]["hint"])
        return report

    def test_list_uses_live_catalog_and_preserves_metadata_and_order(self):
        self.assertEqual(self.plan("list"), self.games)
        self.games.append({"id": "future-demo", "title": "Future", "host": "future"})
        self.write_catalog(self.games)
        status, output, errors = self.invoke("list", "--json")
        self.assertEqual((status, errors), (0, ""))
        self.assertEqual(json.loads(output), self.games)
        self.run.assert_not_called()

    def test_human_list_includes_host_alias(self):
        status, output, errors = self.invoke("list")
        self.assertEqual((status, errors), (0, ""))
        self.assertIn("Courtyard  (/owned/)", output)
        self.assertIn("fresh-demo", output)
        self.run.assert_not_called()

    def test_empty_list(self):
        self.write_catalog([])
        self.assertEqual(self.plan("list"), [])
        self.assertIn("No games", self.invoke("list")[1])

    def test_catalog_errors_are_actionable_and_never_launch(self):
        for data in ({}, [None], [{"id": "../escape", "host": "x", "title": "x"}],
                     [{"id": "x", "host": "../escape", "title": "x"}],
                     [{"id": "x", "host": "x", "title": None}],
                     [self.games[0], self.games[0]]):
            with self.subTest(data=data):
                self.write_catalog(data)
                status, _, errors = self.invoke("list")
                self.assertEqual(status, 2)
                self.assertIn("catalog.json", errors)
        self.run.assert_not_called()

    def test_invalid_json_encoding_and_missing_catalog(self):
        path = self.root / "examples" / "catalog.json"
        for content in (b"{invalid", b"\xff"):
            path.write_bytes(content)
            status, _, errors = self.invoke("list")
            self.assertEqual(status, 2)
            self.assertIn("cannot read", errors)
        path.unlink()
        self.assertEqual(self.invoke("run", "fresh-demo")[0], 2)
        self.run.assert_not_called()

    def test_run_defaults_and_literal_game_arguments(self):
        literal = "target/screen shot; $(touch never).png"
        plan = self.plan("run", "fresh-demo", "--", "--screenshot", literal, "--dry-run")
        self.assertEqual(plan["argv"], [
            "cargo", "run", "--offline", "--locked", "--jobs", "2",
            "--package", "fresh-demo", "--profile", "release", "--",
            "--screenshot", literal, "--dry-run",
        ])
        self.assertEqual(plan["cwd"], str(self.root))

    def test_run_options_work_after_game_and_before_game(self):
        for args in (("run", "fresh-demo", "--profile", "dev", "--jobs", "1", "--dry-run"),
                     ("run", "--dry-run", "--profile", "dev", "--jobs", "1", "fresh-demo")):
            with self.subTest(args=args):
                status, output, errors = self.invoke(*args)
                self.assertEqual((status, errors), (0, ""))
                plan = json.loads(output)
                self.assertEqual(plan["argv"][-2:], ["--profile", "dev"])
                self.assertEqual(plan["env"]["CARGO_BUILD_JOBS"], "1")
        self.run.assert_not_called()

    def test_game_dry_run_argument_does_not_disable_launch(self):
        literal = "target/screen shot; $(touch never).png"
        status, _, errors = self.invoke("run", "fresh-demo", "--", "--dry-run", "--screenshot", literal)
        self.assertEqual((status, errors), (0, ""))
        command = self.run.call_args.args[0]
        self.assertEqual(command[-4:], ["--", "--dry-run", "--screenshot", literal])
        self.assertFalse(self.run.call_args.kwargs["shell"])

    def test_unknown_game_host_alias_and_missing_manifest(self):
        for game in ("unknown", "owned", "../fresh-demo", "fresh-demo;echo x"):
            with self.subTest(game=game):
                status, _, errors = self.invoke("run", game)
                self.assertEqual(status, 2)
                self.assertIn("unknown game", errors)
                self.assertIn("list", errors)
        (self.root / "examples" / "fresh-demo" / "Cargo.toml").unlink()
        self.assertIn("Cargo.toml", self.invoke("run", "fresh-demo")[2])
        self.run.assert_not_called()

    def test_build_web_routes_existing_script_and_profile(self):
        for profile in (None, "release", "small"):
            args = ["build-web"] + ([] if profile is None else [profile])
            plan = self.plan(*args)
            self.assertEqual(plan["argv"], [
                sys.executable, "-B", str(self.root / "scripts" / "build-web.py"),
                profile or "release",
            ])
            self.assertEqual(plan["env"], {"CARGO_NET_OFFLINE": "true", "CARGO_BUILD_JOBS": "2"})

    def test_test_and_check_workspace_defaults(self):
        for name in ("test", "check"):
            plan = self.plan(name)
            expected = ["cargo", name, "--offline", "--locked", "--jobs", "2",
                        "--workspace", "--all-features"]
            if name == "check":
                expected.append("--all-targets")
            self.assertEqual(plan["argv"], expected + ["--profile", "dev"])

    def test_targeted_tests_filter_and_harness_flags(self):
        plan = self.plan("test", "-p", "ferravex-core", "--package", "fresh-demo",
                         "--lib", "--profile", "release", "session", "--", "--nocapture")
        self.assertEqual(plan["argv"], [
            "cargo", "test", "--offline", "--locked", "--jobs", "2",
            "--package", "ferravex-core", "--package", "fresh-demo", "--all-features",
            "--lib", "--profile", "release", "session", "--", "--nocapture",
        ])

    def test_non_game_commands_do_not_require_catalog(self):
        (self.root / "examples" / "catalog.json").unlink()
        for command in ("test", "check", "build-web", "doctor", "new"):
            with self.subTest(command=command):
                self.plan(command)

    def test_invalid_arguments_never_launch(self):
        for args in ((), ("deploy",), ("run",), ("check", "--jobs", "0"),
                     ("test", "--jobs", "3"), ("run", "fresh-demo", "--autoplay"),
                     ("test", "-p", "bad/package"), ("check", "--online"),
                     ("build-web", "dev"), ("build-web", "small", "extra"),
                     ("list", "--", "extra"), ("check", "--", "--offline"),
                     ("run", "fresh-demo", "--prof", "dev")):
            with self.subTest(args=args):
                self.assertEqual(self.invoke(*args)[0], 2)
        self.run.assert_not_called()

    def test_helpers_forward_arguments_in_order_and_help(self):
        for name, script, args in (
            ("doctor", "doctor.py", ["--json", "--profile", "core", "--timeout", "2"]),
            ("new", "new-game.py", ["my-game", "dir with spaces", "--template", "2d"]),
            ("new", "new-game.py", ["--future-option", "x", "my-game"]),
            ("doctor", "doctor.py", ["--help"]),
            ("new", "new-game.py", ["--help"]),
        ):
            with self.subTest(name=name, args=args):
                plan = self.plan(name, *args)
                self.assertEqual(plan["argv"], [sys.executable, "-B", str(self.root / "scripts" / script), *args])
                self.assertEqual(plan["cwd"], str(Path.cwd() if name == "new" else self.root))

    def test_new_launch_preserves_caller_directory_and_relative_destination(self):
        caller = self.root / "examples"
        with patch.object(cli.Path, "cwd", return_value=caller):
            status, _, errors = self.invoke("new", "my-game", "../my game", "--template", "3d")
        self.assertEqual((status, errors), (0, ""))
        self.assertEqual(self.run.call_args.kwargs["cwd"], caller)
        self.assertEqual(self.run.call_args.args[0][-4:], ["my-game", "../my game", "--template", "3d"])

    def test_helper_separator_and_dry_run_are_unambiguous(self):
        plan = self.plan("new", "my-game", "--", "--dry-run", "--help")
        self.assertEqual(plan["argv"][-4:], ["my-game", "--", "--dry-run", "--help"])
        status, output, errors = self.invoke("doctor", "--json", "--dry-run")
        self.assertEqual((status, errors), (0, ""))
        self.assertEqual(json.loads(output)["argv"][-1], "--json")
        self.run.assert_not_called()

    def test_missing_helpers_fail_without_subprocess(self):
        for name, script in (("doctor", "doctor.py"), ("new", "new-game.py"), ("build-web", "build-web.py")):
            (self.root / "scripts" / script).unlink()
            with self.subTest(name=name):
                status, _, errors = self.invoke(name)
                self.assertEqual(status, 2)
                self.assertIn(f"missing helper {self.root / 'scripts' / script}", errors)
        self.run.assert_not_called()

    def test_execution_preserves_environment_and_overrides_cargo_limits(self):
        with patch.dict(os.environ, {"CARGO_BUILD_JOBS": "64", "CARGO_NET_OFFLINE": "false", "TEST_KEEP": "yes"}):
            status, _, _ = self.invoke("check", "--jobs", "1")
        self.assertEqual(status, 0)
        _, kwargs = self.run.call_args
        self.assertEqual(kwargs["cwd"], self.root)
        self.assertFalse(kwargs["shell"])
        self.assertFalse(kwargs["check"])
        self.assertEqual(kwargs["env"]["TEST_KEEP"], "yes")
        self.assertEqual(kwargs["env"]["CARGO_NET_OFFLINE"], "true")
        self.assertEqual(kwargs["env"]["CARGO_BUILD_JOBS"], "1")

    def test_child_status_propagation_including_helpers_and_signals(self):
        for command in ("check", "test", "build-web", "doctor", "new"):
            for child_status, expected in ((0, 0), (7, 7), (101, 101), (-15, 143)):
                with self.subTest(command=command, child_status=child_status):
                    self.run.return_value = subprocess.CompletedProcess([], child_status)
                    self.assertEqual(self.invoke(command)[0], expected)

    def test_launch_failures_and_interrupt(self):
        for error, expected in ((FileNotFoundError("missing"), 127),
                                (PermissionError("denied"), 126), (OSError("bad executable"), 1),
                                (KeyboardInterrupt(), 130)):
            with self.subTest(error=error):
                self.run.side_effect = error
                status, _, errors = self.invoke("check")
                self.assertEqual(status, expected)
                if expected != 130:
                    self.assertIn("cargo", errors)

    def test_root_help_does_not_require_catalog_or_helpers(self):
        (self.root / "examples" / "catalog.json").unlink()
        status, output, errors = self.invoke("--help")
        self.assertEqual((status, errors), (0, ""))
        for command in ("list", "run", "build-web", "test", "check", "doctor", "new"):
            self.assertIn(command, output)
        self.run.assert_not_called()

    def test_structured_dry_run_preserves_original_plan_for_every_child_route(self):
        for args in (
            ("run", "fresh-demo", "--", "--result-json", "--dry-run"),
            ("test", "-p", "ferravex-core", "--jobs", "1", "session", "--", "--nocapture"),
            ("check", "-p", "fresh-demo"), ("build-web", "small"),
            ("doctor", "--json"), ("new", "my-game", "dir with spaces"),
        ):
            with self.subTest(args=args):
                expected = self.plan(*args)
                report = self.outcome("--dry-run", *args)
                self.assertEqual(report["command"], args[0])
                self.assertEqual(report["status"], "dry_run")
                self.assertTrue(report["dry_run"])
                self.assertEqual(report["plan"], expected)
                self.assertIsNone(report["child_returncode"])
                self.assertIsNone(report["error"])
                self.assertIsNone(report["data"])
        self.run.assert_not_called()

    def test_structured_list_retains_catalog_and_has_no_child_plan(self):
        for args in (("list",), ("list", "--json"), ("list", "--dry-run")):
            with self.subTest(args=args):
                report = self.outcome(*args)
                self.assertEqual(report["data"], self.games)
                self.assertEqual(report["status"], "dry_run" if "--dry-run" in args else "ok")
                self.assertIsNone(report["child_returncode"])
                self.assertIsNone(report["plan"])
        self.run.assert_not_called()

    def test_result_flag_works_before_or_after_subcommand_and_not_after_separator(self):
        for args in (("--result-json", "list"), ("list", "--result-json"),
                     ("--result-json", "list", "--result-json")):
            with self.subTest(args=args):
                status, output, errors = self.invoke(*args)
                self.assertEqual((status, errors), (0, ""))
                self.assertEqual(json.loads(output)["data"], self.games)
        for args in (("run", "fresh-demo"), ("test",), ("doctor",), ("new",)):
            with self.subTest(args=args):
                status, output, errors = self.invoke(*args, "--", "--result-json")
                self.assertEqual((status, output, errors), (0, "", ""))
                self.assertEqual(self.run.call_args.args[0][-2:], ["--", "--result-json"])
                self.assertNotIn("stdout", self.run.call_args.kwargs)

    def test_structured_argument_errors_include_help_without_launching(self):
        for args, command in (
            ((), None), (("deploy",), None), (("run",), "run"),
            (("check", "--jobs", "0"), "check"), (("check", "--jobs", "3"), "check"),
            (("check", "--jobs", "many"), "check"), (("check", "--jobs"), "check"),
            (("test", "-p", "bad/package"), "test"), (("test", "-p"), "test"),
            (("build-web", "dev"), "build-web"), (("check", "--profile", "fast"), "check"),
        ):
            with self.subTest(args=args):
                report = self.outcome(*args, expected=2)
                self.assertEqual(report["command"], command)
                self.assertEqual(report["status"], "error")
                self.assertEqual(report["error"]["code"], "invalid_arguments")
                self.assertIn("--help", report["error"]["hint"])
                self.assertIn("usage:", report["error"]["details"]["usage"])
                self.assertIsNone(report["plan"])
        self.run.assert_not_called()

    def test_unknown_arguments_suggest_separator_and_preserve_tokens(self):
        for command, args in (("run", ("fresh-demo",)), ("test", ())):
            with self.subTest(command=command):
                report = self.outcome(command, *args, "--autopaly", "--limit=5", expected=2)
                self.assertEqual(report["error"]["code"], "unknown_arguments")
                self.assertEqual(report["error"]["details"]["arguments"], ["--autopaly", "--limit=5"])
                self.assertIn("after --", report["error"]["hint"])
                self.assertIn(f"{command} --help", report["error"]["hint"])
        status, _, errors = self.invoke("run", "fresh-demo", "--autoplay")
        self.assertEqual(status, 2)
        self.assertIn("after --", errors)
        self.run.assert_not_called()

    def test_unknown_package_is_checked_before_execution_even_in_dry_run(self):
        for command in ("test", "check"):
            for flags in ((), ("--dry-run",)):
                with self.subTest(command=command, flags=flags):
                    report = self.outcome(command, "-p", "ferravex-core", "-p", "ferravex-cor",
                                          *flags, expected=2)
                    error = report["error"]
                    self.assertEqual(error["code"], "unknown_package")
                    self.assertEqual(error["details"]["value"], "ferravex-cor")
                    self.assertIn("ferravex-core", error["details"]["suggestions"])
                    self.assertEqual(error["details"]["choices"], ["ferravex-core", "fresh-demo", "owned-lab"])
                    self.assertIsNone(report["plan"])
                    self.assertEqual(report["dry_run"], bool(flags))
        status, _, errors = self.invoke("check", "-p", "ferravex-cor")
        self.assertEqual(status, 2)
        self.assertIn("unknown package", errors)
        self.assertIn("Did you mean 'ferravex-core'", errors)
        self.assertIn("--package", errors)
        report = self.outcome("check", "-p", "zzzzzz", expected=2)
        self.assertEqual(report["error"]["details"]["suggestions"], [])
        self.assertIn("Available packages:", report["error"]["hint"])
        self.run.assert_not_called()

    def test_unknown_game_reports_suggestions_including_host_alias(self):
        for name, suggestion in (("fresh-dem", "fresh-demo"), ("owned", "owned-lab")):
            with self.subTest(name=name):
                error = self.outcome("run", name, expected=2)["error"]
                self.assertEqual(error["code"], "unknown_game")
                self.assertEqual(error["details"]["suggestions"][0], suggestion)
                self.assertIn("ferravex.py list", error["hint"])
        self.run.assert_not_called()

    def test_package_discovery_uses_declared_names_and_excludes_not_catalog(self):
        self.write_workspace(["examples/*", "crates/*", "crates/core-directory"],
                             exclude=["examples/owned-*"], root_package="root-package")
        (self.root / "examples" / "catalog.json").unlink()
        packages = cli.load_packages(self.root)
        self.assertEqual(packages, ["ferravex-core", "fresh-demo", "root-package"])
        self.assertIn("ferravex-core", self.plan("test", "-p", "ferravex-core")["argv"])
        report = self.outcome("check", "-p", "core-directory", expected=2)
        self.assertEqual(report["error"]["code"], "unknown_package")
        report = self.outcome("check", "-p", "owned-lab", expected=2)
        self.assertNotIn("owned-lab", report["error"]["details"]["choices"])
        self.run.assert_not_called()

    def test_package_discovery_refreshes_manifest_names_without_cargo(self):
        path = self.root / "crates" / "core-directory" / "Cargo.toml"
        self.assertIn("ferravex-core", cli.load_packages(self.root))
        path.write_text('[package]\nname = "new-core"\n', encoding="utf-8")
        self.assertEqual(cli.load_packages(self.root), ["fresh-demo", "new-core", "owned-lab"])
        self.assertIn("new-core", self.plan("check", "-p", "new-core")["argv"])
        self.run.assert_not_called()

    def test_invalid_workspace_is_actionable_and_never_launches(self):
        path = self.root / "Cargo.toml"
        for content in (b"invalid toml", b"\xff", b'[workspace]\nmembers = "bad"',
                        b'[workspace]\nmembers = ["../escape"]',
                        b'[workspace]\nmembers = ["missing-member"]', b'[package]\nname = "root"'):
            with self.subTest(content=content):
                path.write_bytes(content)
                report = self.outcome("check", "-p", "ferravex-core", expected=2)
                self.assertEqual(report["error"]["code"], "invalid_workspace")
                self.assertIn("Cargo.toml", report["error"]["message"])
        path.unlink()
        self.assertEqual(self.outcome("check", "-p", "ferravex-core", expected=2)["error"]["code"],
                         "invalid_workspace")
        self.run.assert_not_called()

    def test_invalid_or_missing_member_manifest_is_not_an_unknown_package(self):
        path = self.root / "crates" / "core-directory" / "Cargo.toml"
        for content in ('[package]\nname = "fresh-demo"', '[package]\nname = 4',
                        '[package]\nname = "bad/name"', '[package]\n', '[package'):
            with self.subTest(content=content):
                path.write_text(content, encoding="utf-8")
                report = self.outcome("test", "-p", "ferravex-core", expected=2)
                self.assertEqual(report["error"]["code"], "invalid_workspace")
                self.assertIn(str(path), report["error"]["message"])
        path.unlink()
        self.assertEqual(self.outcome("test", "-p", "ferravex-core", expected=2)["error"]["code"],
                         "invalid_workspace")
        self.run.assert_not_called()

    def test_structured_catalog_and_missing_file_errors(self):
        self.write_catalog([None])
        self.assertEqual(self.outcome("list", expected=2)["error"]["code"], "invalid_catalog")
        self.write_catalog(self.games)
        (self.root / "examples" / "fresh-demo" / "Cargo.toml").unlink()
        self.assertEqual(self.outcome("run", "fresh-demo", expected=2)["error"]["code"],
                         "missing_manifest")
        for command, helper in (("doctor", "doctor.py"), ("new", "new-game.py"),
                                ("build-web", "build-web.py")):
            with self.subTest(command=command):
                (self.root / "scripts" / helper).unlink()
                report = self.outcome(command, expected=2)
                self.assertEqual(report["error"]["code"], "missing_helper")
                self.assertEqual(report["error"]["details"]["path"], str(self.root / "scripts" / helper))
        self.run.assert_not_called()

    def test_structured_catalog_rejects_nonfinite_numbers_instead_of_emitting_invalid_json(self):
        path = self.root / "examples" / "catalog.json"
        for number in ("NaN", "Infinity", "-Infinity", "1e999"):
            with self.subTest(number=number):
                path.write_text(
                    '[{"id":"demo","host":"demo","title":"Demo","metadata":{"score":'
                    + number + '}}]', encoding="utf-8",
                )
                error = self.outcome("list", expected=2)["error"]
                self.assertEqual(error["code"], "invalid_catalog")
                self.assertIn("non-finite", error["message"])
        self.run.assert_not_called()

    def test_structured_new_handles_missing_caller_directory(self):
        with patch.object(cli.Path, "cwd", side_effect=FileNotFoundError("deleted working directory")):
            report = self.outcome("new", "my-game", "destination", expected=2)
        self.assertEqual(report["error"]["code"], "invalid_workspace")
        self.assertIn("working directory", report["error"]["hint"])
        self.assertIsNone(report["plan"])
        self.run.assert_not_called()

    def test_structured_help_does_not_launch_or_require_workspace(self):
        (self.root / "Cargo.toml").unlink()
        (self.root / "examples" / "catalog.json").unlink()
        for args, command in ((("--help",), None), (("test", "--help"), "test")):
            with self.subTest(args=args):
                report = self.outcome(*args)
                self.assertEqual(report["command"], command)
                self.assertEqual(report["status"], "help")
                self.assertIn("--result-json", report["data"]["help"])
                self.assertIsNone(report["plan"])
                self.assertIsNone(report["error"])
        self.run.assert_not_called()

    def test_structured_child_results_propagate_exit_status_for_all_routes(self):
        for args in (("run", "fresh-demo"), ("test",), ("check",), ("build-web",),
                     ("doctor", "--json"), ("new", "--help")):
            for child_status, expected in ((0, 0), (2, 2), (7, 7), (101, 101), (-15, 143)):
                with self.subTest(args=args, child_status=child_status):
                    self.run.return_value = subprocess.CompletedProcess([], child_status)
                    report = self.outcome(*args, expected=expected)
                    self.assertEqual(report["status"], "error" if expected else "ok")
                    self.assertEqual(report["child_returncode"], child_status)
                    self.assertFalse(report["dry_run"])
                    self.assertIsNone(report["data"])
                    self.assertEqual(report["plan"]["argv"], self.run.call_args.args[0])
                    self.assertIn("stdout", self.run.call_args.kwargs)
                    self.assertNotIn("capture_output", self.run.call_args.kwargs)
                    if expected:
                        self.assertEqual(report["error"]["code"],
                                         "child_signaled" if child_status < 0 else "child_failed")
                    else:
                        self.assertIsNone(report["error"])

    def test_structured_launch_failures_and_interrupt_have_stable_codes(self):
        for error, expected, code in (
            (FileNotFoundError("missing"), 127, "executable_not_found"),
            (PermissionError("denied"), 126, "permission_denied"),
            (OSError("bad executable"), 1, "launch_failed"),
            (KeyboardInterrupt(), 130, "interrupted"),
        ):
            with self.subTest(error=error):
                self.run.side_effect = error
                report = self.outcome("check", expected=expected)
                self.assertEqual(report["status"], "error")
                self.assertEqual(report["error"]["code"], code)
                self.assertIsNone(report["child_returncode"])
                self.assertIsNotNone(report["plan"])

    def test_structured_results_keep_limits_and_do_not_expose_inherited_environment(self):
        with patch.dict(os.environ, {"CARGO_BUILD_JOBS": "64", "CARGO_NET_OFFLINE": "false",
                                     "CLI_TEST_SECRET": "not-for-the-report"}):
            for args in (("check",), ("test", "--jobs", "1"), ("run", "fresh-demo"),
                         ("build-web",), ("doctor",), ("new",)):
                with self.subTest(args=args):
                    report = self.outcome(*args)
                    overrides = report["plan"]["env"]
                    self.assertEqual(set(overrides), {"CARGO_BUILD_JOBS", "CARGO_NET_OFFLINE"})
                    self.assertIn(overrides["CARGO_BUILD_JOBS"], ("1", "2"))
                    self.assertEqual(overrides["CARGO_NET_OFFLINE"], "true")
                    self.assertNotIn("not-for-the-report", json.dumps(report))
                    self.assertEqual(self.run.call_args.kwargs["env"]["CLI_TEST_SECRET"], "not-for-the-report")


class ProcessTests(unittest.TestCase):
    """Verify actual stdout/stderr separation with tiny disposable Python helpers."""

    def test_real_child_output_cannot_contaminate_result_or_change_exit_status(self):
        source = Path(__file__).with_name("ferravex.py")
        with tempfile.TemporaryDirectory(prefix="ferravex process ") as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            (scripts / "ferravex.py").write_bytes(source.read_bytes())
            (scripts / "doctor.py").write_text(
                'import os, sys\n'
                'os.write(1, b"child stdout {invalid JSON}\\n\\xff")\n'
                'os.write(2, b"child stderr\\n")\n'
                'print("jobs=" + os.environ["CARGO_BUILD_JOBS"])\n'
                'print("offline=" + os.environ["CARGO_NET_OFFLINE"])\n'
                'print("args=" + repr(sys.argv[1:]))\n'
                'raise SystemExit(7)\n', encoding="utf-8",
            )
            result = subprocess.run(
                [sys.executable, "-B", str(scripts / "ferravex.py"), "doctor", "--json", "--result-json"],
                capture_output=True, check=False, timeout=10,
                env={**os.environ, "CARGO_BUILD_JOBS": "64", "CARGO_NET_OFFLINE": "false"},
            )
            self.assertEqual(result.returncode, 7)
            report = json.loads(result.stdout)
            self.assertEqual(report["exit_code"], 7)
            self.assertEqual(report["child_returncode"], 7)
            self.assertEqual(report["error"]["code"], "child_failed")
            self.assertIn(b"child stdout {invalid JSON}", result.stderr)
            self.assertIn(b"\xff", result.stderr)
            self.assertIn(b"child stderr", result.stderr)
            self.assertIn(b"jobs=2", result.stderr)
            self.assertIn(b"offline=true", result.stderr)
            self.assertIn(b"args=['--json']", result.stderr)

    def test_real_parser_error_is_one_json_document_with_exit_two(self):
        result = subprocess.run(
            [sys.executable, "-B", str(Path(__file__).with_name("ferravex.py")),
             "check", "--jobs", "3", "--result-json"],
            capture_output=True, text=True, check=False, timeout=10,
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stderr, "")
        report = json.loads(result.stdout)
        self.assertEqual(report["command"], "check")
        self.assertEqual(report["exit_code"], 2)
        self.assertEqual(report["error"]["code"], "invalid_arguments")


if __name__ == "__main__":
    unittest.main()
