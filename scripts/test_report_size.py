"""Size policy and CLI regressions, using temporary artifacts and stdlib only."""

import contextlib
import copy
import doctest
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("report-size.py")
spec = importlib.util.spec_from_file_location("report_size", SCRIPT)
report_size = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report_size)


class SizeReportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ferravex size ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.target = self.root / "build output"
        for patcher in (
            patch.object(report_size, "ROOT", self.root),
            patch.object(report_size, "GAMES", [{"id": "test-game", "host": "test-host"}]),
            patch.object(report_size, "EXAMPLES", ("test-game",)),
            patch.object(report_size.subprocess, "check_output",
                         return_value=json.dumps({"target_directory": str(self.target)})),
        ):
            mocked = patcher.start()
            self.addCleanup(patcher.stop)
            if patcher.attribute == "check_output":
                self.metadata = mocked
        self.wasm = self.write("build output/wasm32-unknown-unknown/release/test_game.wasm",
                               b"\x00asm\x01\x00\x00\x00" + b"test module" * 100)
        self.label = "wasm/test_game.wasm"
        self.candidates = [(self.label, self.wasm)]

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def policy(self, **limits):
        return {"schema": 1, "artifacts": {self.label: limits}}

    def collect(self, **options):
        return report_size.collect_report(self.candidates, **options)

    def cli(self, *args):
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                status = report_size.main(list(args))
            except SystemExit as error:
                status = error.code
        return status, stdout.getvalue(), stderr.getvalue()

    def test_streamed_sizes_match_original_gzip_including_chunk_boundaries(self):
        rng = random.Random(108)
        for data in (b"", b"a", b"a" * (report_size.CHUNK_BYTES * 3),
                     rng.randbytes(report_size.CHUNK_BYTES),
                     rng.randbytes(report_size.CHUNK_BYTES * 3 + 1)):
            with self.subTest(length=len(data)):
                self.wasm.write_bytes(data)
                expected = {"bytes": len(data), "gzip9_bytes": len(gzip.compress(data, 9, mtime=0))}
                self.assertEqual(report_size.measure_artifact(self.wasm), expected)
                os.utime(self.wasm, (100, 100))
                self.assertEqual(report_size.measure_artifact(self.wasm), expected)

    def test_streaming_reads_are_bounded(self):
        class BoundedReader(io.BytesIO):
            def read(self, count=-1):
                self_test.assertEqual(count, report_size.CHUNK_BYTES)
                return super().read(count)

        self_test = self
        with patch.object(Path, "open", return_value=BoundedReader(b"x" * 200000)):
            self.assertEqual(report_size.measure_artifact(self.wasm)["bytes"], 200000)

    def test_existing_json_keys_and_stable_order_are_preserved(self):
        self.candidates += [("web/z.js", self.wasm), ("native/a", self.wasm)]
        first = self.collect()
        self.candidates.reverse()
        self.assertEqual(self.collect(), first)
        self.assertEqual(first["profile"], "release")
        self.assertEqual(first["missing"], [])
        self.assertEqual(first["artifacts"][0]["artifact"], "native/a")
        self.assertEqual(first["artifacts"][0]["path"], str(self.wasm.relative_to(self.root)))
        self.assertEqual(first, json.loads(json.dumps(first, allow_nan=False)))
        with patch.object(report_size, "ROOT", self.root / "elsewhere"):
            self.assertEqual(self.collect()["artifacts"][0]["path"], str(self.wasm))

    def test_partial_inventory_stays_optional_but_strict_missing_fails(self):
        self.candidates.append(("native/missing", self.root / "missing"))
        report = self.collect()
        self.assertTrue(report["ok"])
        self.assertEqual(report["missing"], ["native/missing"])
        self.assertFalse(self.collect(require_complete=True)["ok"])
        self.assertTrue(self.collect(artifacts=[self.label], require_complete=True)["ok"])

    def test_explicit_unknown_or_missing_selection_fails(self):
        for label in ("wasm/missing.wasm", "web/typo.js"):
            with self.subTest(label=label):
                report = self.collect(artifacts=[self.label, label])
                self.assertFalse(report["ok"])
                self.assertEqual(report["missing"], [label])
                self.assertTrue(any(label in failure for failure in report["failures"]))

    def test_no_artifacts_is_a_failure_not_an_empty_success(self):
        self.wasm.unlink()
        report = self.collect()
        self.assertFalse(report["ok"])
        self.assertEqual(report["artifacts"], [])
        self.assertEqual(report["missing"], [self.label])
        self.assertIn("no built artifacts", report["failures"][0])

    def test_directory_and_broken_symlink_are_not_measured_as_files(self):
        self.wasm.unlink()
        for kind in ("directory", "symlink"):
            with self.subTest(kind=kind):
                if kind == "directory":
                    self.wasm.mkdir()
                else:
                    self.wasm.rmdir()
                    self.wasm.symlink_to(self.root / "absent")
                self.assertEqual(self.collect()["missing"], [self.label])

    def test_native_exe_fallback_and_target_profile_discovery(self):
        exe = self.write("build output/x86_64-pc-windows-msvc/small/test-game.exe", b"native")
        self.write("dist/web/test-host/index.html", b"host")
        self.write("dist/web/test-host/game.wasm", b"staged wasm is not counted twice")
        candidates = report_size._candidates(self.target, "small", "x86_64-pc-windows-msvc")
        paths = dict(candidates)
        self.assertEqual(paths["native/test-game"].with_suffix(".exe"), exe)
        self.assertEqual(paths[self.label],
                         self.target / "wasm32-unknown-unknown/small/test_game.wasm")
        self.assertIn("native/ferravex-server", paths)
        self.assertIn("web/runtime.js", paths)
        self.assertIn("web/test-host/index.html", paths)
        self.assertNotIn("web/test-host/game.wasm", paths)
        report = report_size.collect_report(candidates, profile="small",
                                            native_target="x86_64-pc-windows-msvc",
                                            artifacts=["native/test-game"])
        self.assertTrue(report["ok"])
        self.assertTrue(report["artifacts"][0]["path"].endswith(".exe"))

    def test_io_errors_fail_and_preserve_other_measurements(self):
        self.candidates.append(("web/other.js", self.wasm))
        original = report_size.measure_artifact
        with patch.object(report_size, "measure_artifact",
                          side_effect=[PermissionError("denied"), original(self.wasm)]):
            report = self.collect()
        self.assertFalse(report["ok"])
        self.assertEqual(len(report["artifacts"]), 1)
        self.assertEqual(report["missing"], [])
        self.assertEqual(report["errors"], [{"artifact": self.label, "message": "denied"}])
        with patch.object(Path, "stat", side_effect=PermissionError("cannot stat")):
            self.assertEqual(len(self.collect()["errors"]), 2)

    def test_exact_and_shrinking_baselines_pass_with_signed_deltas(self):
        baseline = self.collect()
        untouched = copy.deepcopy(baseline)
        report = self.collect(baseline=baseline)
        self.assertTrue(report["ok"])
        self.assertEqual(report["comparison"]["artifacts"][0]["bytes"]["delta"], 0)
        self.wasm.write_bytes(b"")
        report = self.collect(baseline=baseline)
        self.assertTrue(report["ok"])
        self.assertLess(report["comparison"]["artifacts"][0]["bytes"]["delta"], 0)
        self.assertEqual(baseline, untouched)

    def test_raw_and_compressed_regressions_are_independently_checked(self):
        for metric in report_size.METRICS:
            with self.subTest(metric=metric):
                baseline = self.collect()
                baseline["artifacts"][0][metric] -= 1
                report = self.collect(baseline=baseline)
                self.assertFalse(report["ok"])
                self.assertEqual(len(report["failures"]), 1)
                self.assertIn(self.label + "/" + metric, report["failures"][0])

    def test_zero_baseline_has_exact_deltas_and_no_division(self):
        baseline = self.collect()
        baseline["artifacts"][0]["bytes"] = 0
        report = self.collect(baseline=baseline)
        self.assertFalse(report["ok"])
        self.assertEqual(report["comparison"]["artifacts"][0]["bytes"]["limit"], 0)
        json.dumps(report, allow_nan=False)

    def test_baseline_added_removed_and_disappearing_glob_artifacts_fail(self):
        baseline = self.collect()
        self.candidates.append(("web/extra.js", self.wasm))
        report = self.collect(baseline=baseline)
        self.assertFalse(report["ok"])
        self.assertEqual(report["comparison"]["added"], ["web/extra.js"])
        baseline = self.collect()
        self.candidates.pop()
        report = self.collect(baseline=baseline)
        self.assertFalse(report["ok"])
        self.assertEqual(report["comparison"]["removed"], ["web/extra.js"])
        self.wasm.unlink()
        self.assertFalse(self.collect(baseline=baseline)["ok"])

    def test_profile_compression_schema_or_failed_baselines_are_rejected(self):
        for field, value in (
            ("profile", "small"), ("schema", 2), ("schema", True), ("schema", None),
            ("compression", {}), ("compression", {**report_size.COMPRESSION, "mtime": False}),
            ("compression", {**report_size.COMPRESSION, "zlib_version": "other"}),
            ("ok", False), ("ok", 1), ("errors", [{"artifact": self.label}]),
            ("failures", ["failed"]), ("artifacts", []), ("artifacts", {}),
            ("artifacts", [None]), ("missing", None), ("missing", [self.label]),
            ("missing", ["web/a.js", "web/a.js"]),
        ):
            with self.subTest(field=field, value=value):
                baseline = self.collect()
                baseline[field] = value
                with self.assertRaises(ValueError):
                    self.collect(baseline=baseline)
        baseline = self.collect()
        del baseline["schema"]
        with self.assertRaises(ValueError):
            self.collect(baseline=baseline)

    def test_baseline_duplicates_missing_metrics_and_bad_numbers_are_rejected(self):
        baseline = self.collect()
        baseline["artifacts"].append(copy.deepcopy(baseline["artifacts"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.collect(baseline=baseline)
        for metric in report_size.METRICS:
            for value in (-1, True, 1.0, "1", None, float("nan"), float("inf"), 1 << 63):
                with self.subTest(metric=metric, value=value):
                    baseline = self.collect()
                    baseline["artifacts"][0][metric] = value
                    with self.assertRaises(ValueError):
                        self.collect(baseline=baseline)
            baseline = self.collect()
            del baseline["artifacts"][0][metric]
            with self.assertRaises(ValueError):
                self.collect(baseline=baseline)

    def test_native_target_and_host_are_checked_but_wasm_is_host_independent(self):
        baseline = self.collect()
        baseline["native_host"] = {"system": "elsewhere", "machine": "different"}
        self.assertTrue(self.collect(baseline=baseline)["ok"])
        self.candidates = [("native/test-game", self.wasm)]
        baseline = self.collect(native_target="target-a")
        with self.assertRaisesRegex(ValueError, "native_target"):
            self.collect(baseline=baseline, native_target="target-b")
        baseline = self.collect()
        baseline["native_host"]["machine"] = "different"
        with self.assertRaisesRegex(ValueError, "native_host"):
            self.collect(baseline=baseline)

    def test_absolute_budgets_are_inclusive_and_independent(self):
        sizes = report_size.measure_artifact(self.wasm)
        self.assertTrue(self.collect(budgets=self.policy(**sizes))["ok"])
        for metric in report_size.METRICS:
            with self.subTest(metric=metric):
                limits = {**sizes, metric: sizes[metric] - 1}
                report = self.collect(budgets=self.policy(**limits))
                self.assertFalse(report["ok"])
                self.assertEqual(len(report["failures"]), 1)
                self.assertIn(metric, report["failures"][0])
        self.wasm.write_bytes(b"")
        self.assertTrue(self.collect(budgets=self.policy(bytes=0, gzip9_bytes=20))["ok"])

    def test_per_artifact_growth_allowances_and_absolute_caps_both_apply(self):
        baseline = self.collect()
        for metric in report_size.METRICS:
            baseline["artifacts"][0][metric] -= 1
        budgets = self.policy(max_growth_bytes=1, max_growth_gzip9_bytes=1)
        self.assertTrue(self.collect(baseline=baseline, budgets=budgets)["ok"])
        budgets["artifacts"][self.label]["max_growth_gzip9_bytes"] = 0
        self.assertFalse(self.collect(baseline=baseline, budgets=budgets)["ok"])
        budgets["artifacts"][self.label]["max_growth_gzip9_bytes"] = 1
        budgets["artifacts"][self.label]["bytes"] = 0
        self.assertFalse(self.collect(baseline=baseline, budgets=budgets)["ok"])
        with self.assertRaisesRegex(ValueError, "require --baseline"):
            self.collect(budgets=budgets)
        self.candidates.append(("web/second.js", self.wasm))
        baseline = self.collect()
        baseline["artifacts"][1]["bytes"] -= 1
        report = self.collect(baseline=baseline, budgets=self.policy(max_growth_bytes=1000))
        self.assertFalse(report["ok"])
        self.assertIn("web/second.js/bytes", report["failures"][0])

    def test_unknown_missing_and_unselected_budget_entries_fail(self):
        budgets = {"schema": 1, "artifacts": {"web/unknown.js": {"bytes": 100}}}
        self.assertFalse(self.collect(budgets=budgets)["ok"])
        self.candidates.append(("web/unknown.js", self.wasm))
        report = self.collect(budgets=budgets, artifacts=[self.label])
        self.assertFalse(report["ok"])
        self.assertIn("web/unknown.js", report["failures"][0])

    def test_invalid_budget_shapes_numbers_and_typos_are_rejected(self):
        documents = [None, [], {}, {"schema": True, "artifacts": {}},
                     {"schema": 2, "artifacts": {}}, {"schema": 1, "artifacts": []},
                     {"schema": 1, "artifacts": {}}, {**self.policy(bytes=1), "extra": 1},
                     self.policy(), self.policy(gzip_bytes=100)]
        documents += [self.policy(bytes=value) for value in
                      (-1, True, 1.0, "1", None, float("inf"), float("nan"), 1 << 63)]
        for document in documents:
            with self.subTest(document=document), self.assertRaises(ValueError):
                report_size.validate_budgets(document)

    def test_invalid_labels_duplicate_candidates_and_bounds_are_checked(self):
        for label in ("", None, 123, "a", "/wasm/a", "wasm/../a", "wasm//a",
                      "wasm/./a", "wasm/a\n", "web/a\\b", "wasm/" + "a" * 1024):
            with self.subTest(label=label), self.assertRaises(ValueError):
                report_size.collect_report([(label, self.wasm)])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            report_size.collect_report(self.candidates * 2)
        with patch.object(report_size, "MAX_ARTIFACTS", 1):
            with self.assertRaises(ValueError):
                report_size.collect_report(self.candidates + [("web/b.js", self.wasm)])
            with self.assertRaises(ValueError):
                self.collect(artifacts=[self.label, "web/b.js"])

    def test_json_reader_rejects_duplicates_nonfinite_invalid_and_oversized_input(self):
        for payload in (b'{"schema":1,"schema":1}', b'{"bytes":NaN}',
                        b'{"bytes":Infinity}', b'{"bytes":-Infinity}', b'{', b'\xff'):
            with self.subTest(payload=payload[:50]):
                path = self.write("bad.json", payload)
                with self.assertRaises(ValueError):
                    report_size._read_json(path)
        path = self.write("oversized.json", b" " * 20)
        with patch.object(report_size, "MAX_JSON_BYTES", 10), self.assertRaises(ValueError):
            report_size._read_json(path)
        with patch.object(report_size.json, "loads", side_effect=RecursionError("too deep")):
            with self.assertRaisesRegex(ValueError, "invalid JSON"):
                report_size._read_json(path)

    def test_cli_baseline_round_trip_and_budget_failure_preserve_json(self):
        status, output, error = self.cli("--artifact", self.label, "--json")
        self.assertEqual((status, error), (0, ""))
        baseline = self.write("baseline.json", output.encode())
        status, output, error = self.cli("--artifact", self.label,
                                         "--baseline", str(baseline), "--json")
        self.assertEqual((status, error), (0, ""))
        self.assertTrue(json.loads(output)["ok"])
        budget = self.write("budgets.json", json.dumps(self.policy(bytes=0)).encode())
        status, output, error = self.cli("--artifact", self.label, "--baseline", str(baseline),
                                         "--budgets", str(budget), "--json")
        self.assertEqual(status, 1)
        self.assertFalse(json.loads(output)["ok"])
        self.assertIn("FAIL", error)
        self.assertEqual(report_size._read_json(baseline)["comparison"], None)
        command = self.metadata.call_args.args[0]
        self.assertEqual(command, ["cargo", "metadata", "--offline", "--no-deps",
                                   "--locked", "--format-version", "1"])

    def test_cli_missing_failure_json_and_human_deltas(self):
        baseline = self.write("baseline.json", json.dumps(self.collect()).encode())
        status, output, error = self.cli("--artifact", self.label, "--baseline", str(baseline))
        self.assertEqual(status, 0)
        self.assertIn("+0 / +0", output)
        self.assertIn("transfer estimates", output)
        self.wasm.unlink()
        status, output, error = self.cli("--artifact", self.label, "--json")
        self.assertEqual(status, 1)
        self.assertEqual(json.loads(output)["missing"], [self.label])
        self.assertIn("no built artifacts", error)

    def test_cli_invalid_policies_fail_before_cargo_and_never_traceback(self):
        for flag in ("--baseline", "--budgets"):
            for payload in (b"null", b"[]", b"{}", b"{", b'{"schema":1,"schema":1}'):
                with self.subTest(flag=flag, payload=payload):
                    path = self.write("bad.json", payload)
                    self.metadata.reset_mock()
                    status, output, error = self.cli(flag, str(path), "--json")
                    self.assertEqual(status, 2)
                    self.assertEqual(output, "")
                    self.assertNotIn("Traceback", error)
                    self.metadata.assert_not_called()
        status, _, error = self.cli("--baseline", str(self.root / "missing.json"))
        self.assertEqual(status, 2)
        self.assertNotIn("Traceback", error)

    def test_cli_metadata_failure_is_actionable(self):
        for result in (FileNotFoundError("cargo absent"),
                       subprocess.CalledProcessError(1, "cargo")):
            with self.subTest(result=result):
                self.metadata.side_effect = result
                status, _, error = self.cli("--json")
                self.assertEqual(status, 2)
                self.assertNotIn("Traceback", error)

    def test_public_doc_examples(self):
        self.assertEqual(doctest.testmod(report_size).failed, 0)


class EntryPointTests(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "temporary Cargo launcher uses a POSIX shebang")
    def test_process_exit_status_and_json_for_real_temporary_files(self):
        with tempfile.TemporaryDirectory(prefix="ferravex size cli ") as directory:
            root = Path(directory)
            script = root / "scripts/report-size.py"
            script.parent.mkdir()
            script.write_bytes(SCRIPT.read_bytes())
            catalog = root / "examples/catalog.json"
            catalog.parent.mkdir()
            catalog.write_text('[{"id":"test-game","host":"test-host"}]', encoding="utf-8")
            target = root / "target"
            wasm = target / "wasm32-unknown-unknown/release/test_game.wasm"
            wasm.parent.mkdir(parents=True)
            wasm.write_bytes(b"\x00asm\x01\x00\x00\x00")
            cargo = root / "bin/cargo"
            cargo.parent.mkdir()
            cargo.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({{'target_directory': {str(target)!r}}}))\n",
                encoding="utf-8",
            )
            cargo.chmod(0o755)
            command = [sys.executable, "-B", str(script),
                       "--artifact", "wasm/test_game.wasm", "--json"]
            options = {"capture_output": True, "text": True, "check": False,
                       "env": {**os.environ, "PATH": str(cargo.parent)}}
            success = subprocess.run(command, **options)
            self.assertEqual(success.returncode, 0, success.stderr)
            self.assertTrue(json.loads(success.stdout)["ok"])
            wasm.unlink()
            failure = subprocess.run(command, **options)
            self.assertEqual(failure.returncode, 1, failure.stderr)
            self.assertEqual(json.loads(failure.stdout)["missing"], ["wasm/test_game.wasm"])
            self.assertIn("FAIL", failure.stderr)

    def test_help_runs_without_cargo_or_built_artifacts(self):
        result = subprocess.run([sys.executable, "-B", str(SCRIPT), "--help"],
                                capture_output=True, text=True,
                                env={**os.environ, "PATH": ""}, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        for flag in ("--baseline", "--budgets", "--artifact", "--require-complete"):
            self.assertIn(flag, result.stdout)
        self.assertIn("max_growth_gzip9_bytes", result.stdout)


if __name__ == "__main__":
    unittest.main()
