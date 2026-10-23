"""Temporary-fixture web builds; never compile Cargo or regenerate shared dist."""

import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent


def load_script(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


builder = load_script("build-owned.py")
web = load_script("build-web.py")


def u32(value):
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def section(kind, data):
    return bytes([kind]) + u32(len(data)) + data


def wasm(exports=()):
    """Valid tiny Wasm with named aliases of one empty function, not game code."""
    names = sorted(exports)
    entries = b"".join(u32(len(name.encode())) + name.encode() + b"\0\0" for name in names)
    return (b"\0asm\x01\0\0\0"
            + section(1, b"\x01\x60\0\0")
            + section(3, b"\x01\0")
            + section(0, b"\x04note ferravex_content_source is not an export")
            + section(7, u32(len(names)) + entries)
            + section(10, b"\x01\x02\0\x0b"))


class BuildFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="journey200-109-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "checkout 'with spaces'"
        self.root.mkdir()
        self.output = self.root / "dist/web"
        self.games = [
            {"id": "scene-game", "host": "scene", "editable_scene": "examples/scene-game/map.fxscene"},
            {"id": "zeta-game", "host": "zeta"},
            {"id": "alpha-game", "host": "alpha"},
        ]
        self.write_catalog()
        self.write("Cargo.toml", "[workspace]\n")
        self.write("Cargo.lock", "version = 4\n")
        self.write(".gitignore", "/dist/\n/target/\n__pycache__/\n*.pyc\n")
        self.write("scripts/audit-owned.py", "# fixture: audit invocation is mocked\n")
        for name in builder.SHARED_ASSETS:
            self.write("web/" + name, "// " + name + "\n")
        for game in self.games:
            self.write("web/" + game["host"] + "/index.html", "<p>" + game["id"] + "</p>\n")
            self.write("examples/" + game["id"] + "/src/lib.rs", "// fixture game\n")
        self.write("examples/scene-game/map.fxscene", "ferravex_scene 1;\n")
        self.write("web/alpha/assets/palette.bin", bytes(range(256)))
        self.write("web/alpha/assets/build-info.json", "nested host data\n")
        templates = {}
        for group in builder.SHADERS.values():
            for file, name, export in group:
                templates.setdefault(file, []).append(f'pub const {name}: &str = r#"{export}\\\n"#;\n')
        for file, parts in templates.items():
            self.write("crates/ferravex-render/src/" + file, "".join(parts))
        self.commands = []
        self.exports = {game["id"]: builder.CONTENT_EXPORTS if game.get("editable_scene") else ()
                        for game in self.games}
        self.after_cargo = None
        self.skip_artifact = None
        self.fail_package = None

    def write(self, relative, contents):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(contents if isinstance(contents, bytes) else contents.encode())
        return path

    def write_catalog(self):
        self.write("examples/catalog.json", json.dumps(self.games, indent=2) + "\n")

    def fake_run(self, command, **kwargs):
        self.commands.append((command, kwargs))
        self.assertEqual(kwargs["cwd"], self.root)
        self.assertTrue(kwargs["check"])
        if command[0] != "cargo":
            self.assertEqual(command, [sys.executable, "-B", "scripts/audit-owned.py"])
            return subprocess.CompletedProcess(command, 0)
        packages = [command[i + 1] for i, value in enumerate(command) if value == "-p"]
        if self.fail_package in packages:
            raise subprocess.CalledProcessError(19, command)
        folder = command[command.index("--profile") + 1]
        folder = "debug" if folder == "dev" else folder
        for package in packages:
            if package != self.skip_artifact:
                self.write(f"target/{builder.TARGET}/{folder}/{package.replace('-', '_')}.wasm",
                           wasm(self.exports[package]))
        if self.after_cargo:
            self.after_cargo(packages)
        return subprocess.CompletedProcess(command, 0)

    def build(self, profile="release"):
        # Git behavior has its own real-Git fixture tests; Cargo and audit never run.
        with patch.object(builder, "_git", return_value=None), \
                patch.object(builder.subprocess, "run", side_effect=self.fake_run):
            return builder.build(profile, root=self.root)

    def manifest(self):
        return json.loads((self.output / builder.MANIFEST).read_bytes())

    def previous_output(self):
        self.write("dist/web/previous.txt", "previous good output")
        self.write("dist/web/build-info.json", "previous manifest")

    def assert_previous_output(self):
        self.assertEqual((self.output / "previous.txt").read_text(), "previous good output")
        self.assertEqual((self.output / "build-info.json").read_text(), "previous manifest")
        self.assertEqual(sorted(path.name for path in self.output.iterdir()),
                         ["build-info.json", "previous.txt"])
        self.assertEqual(list(self.output.parent.glob(".web-build-*")), [])


class BuildTests(BuildFixture):
    def test_manifest_hashes_complete_staged_bytes_and_preserves_report(self):
        report = self.build()
        info = self.manifest()
        self.assertEqual(info["schema_version"], 1)
        self.assertEqual(info["profile"], "release")
        self.assertEqual(info["target"], "wasm32-unknown-unknown")
        self.assertEqual(info["external_packages"], 0)
        self.assertEqual(list(info["files"]), sorted(info["files"]))
        expected = {p.relative_to(self.output).as_posix() for p in self.output.rglob("*")
                    if p.is_file() and p != self.output / "build-info.json"}
        self.assertEqual(set(info["files"]), expected)
        self.assertIn("alpha/assets/build-info.json", expected)
        for relative, record in info["files"].items():
            data = (self.output / relative).read_bytes()
            self.assertEqual(record, {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
            self.assertNotIn("\\", relative)
        for name in builder.SHARED_ASSETS:
            self.assertEqual((self.output / name).read_bytes(), (self.root / "web" / name).read_bytes())
        self.assertEqual((self.output / "alpha/assets/palette.bin").read_bytes(), bytes(range(256)))
        self.assertEqual((self.output / "games.json").read_bytes(),
                         (self.root / "examples/catalog.json").read_bytes())
        self.assertEqual(report["alpha_game"]["files"]["assets/palette.bin"], 256)
        self.assertEqual(report["alpha_game"]["third_party_runtime_packages"], 0)
        self.assertEqual(report["scene_game"]["wasm_bytes"], len(wasm(builder.CONTENT_EXPORTS)))
        self.assertNotIn(str(self.root), (self.output / "build-info.json").read_text())

    def test_repeat_build_is_byte_identical_despite_order_mtime_and_stale_output(self):
        self.build()
        first = (self.output / "build-info.json").read_bytes()
        # Recreate source files in reverse order, without changing any bytes.
        for path in sorted((self.root / "web").rglob("*"), reverse=True):
            if path.is_file():
                data = path.read_bytes()
                path.unlink()
                path.write_bytes(data)
                os.utime(path, (100, 100))
        self.write("dist/web/retired/stale.js", "must disappear")
        self.build()
        self.assertEqual((self.output / "build-info.json").read_bytes(), first)
        self.assertFalse((self.output / "retired").exists())

    def test_profiles_group_isolation_and_explicit_target_directory(self):
        for profile in ("release", "small", "dev"):
            with self.subTest(profile=profile), patch.dict(os.environ, {"CARGO_TARGET_DIR": "/unused-target"}):
                self.commands.clear()
                self.build(profile)
                commands = [command for command, _ in self.commands if command[0] == "cargo"]
                packages = [[c[i + 1] for i, value in enumerate(c) if value == "-p"] for c in commands]
                self.assertEqual(packages, [["alpha-game", "zeta-game"], ["scene-game"]])
                for command in commands:
                    for flag in ("--offline", "--locked", "--lib"):
                        self.assertIn(flag, command)
                    self.assertEqual(command[command.index("--jobs") + 1], "2")
                    self.assertEqual(command[command.index("--profile") + 1], profile)
                    self.assertEqual(command[command.index("--target-dir") + 1], str(self.root / "target"))
                    self.assertNotIn("--all-features", command)
                info = self.manifest()
                self.assertEqual(info["profile"], profile)
                self.assertEqual(info["games"]["alpha-game"]["variant"], "runtime")
                self.assertEqual(info["games"]["scene-game"]["variant"], "authoring")
                self.assertEqual(info["games"]["scene-game"]["editable_scene"], self.games[0]["editable_scene"])
                self.assertEqual(info["build_groups"], [
                    {"variant": "runtime", "packages": ["alpha-game", "zeta-game"]},
                    {"variant": "authoring", "packages": ["scene-game"]},
                ])

    def test_each_group_is_captured_before_later_cargo_invocations(self):
        def overwrite_runtime(packages):
            if "scene-game" in packages:
                self.write("target/wasm32-unknown-unknown/release/alpha_game.wasm", b"overwritten")
        self.after_cargo = overwrite_runtime
        self.build()
        self.assertEqual((self.output / "alpha/alpha_game.wasm").read_bytes(), wasm())

    def test_empty_group_is_not_a_workspace_wasm_build(self):
        for editing in (False, True):
            with self.subTest(editing=editing):
                self.games = [{"id": "scene-game", "host": "scene"}]
                if editing:
                    self.games[0]["editable_scene"] = "examples/scene-game/map.fxscene"
                self.exports["scene-game"] = builder.CONTENT_EXPORTS if editing else ()
                self.write_catalog()
                self.commands.clear()
                self.build()
                self.assertEqual(len([c for c, _ in self.commands if c[0] == "cargo"]), 1)

    def test_variant_mismatch_preserves_previous_output(self):
        for package, exports, error in (
            ("alpha-game", builder.CONTENT_EXPORTS, "unexpected authoring"),
            ("scene-game", (), "missing authoring"),
            ("scene-game", {"ferravex_content_source"}, "missing authoring"),
        ):
            with self.subTest(package=package, exports=exports):
                self.previous_output()
                before = self.exports[package]
                self.exports[package] = exports
                with self.assertRaisesRegex(ValueError, error):
                    self.build()
                self.exports[package] = before
                self.assert_previous_output()

    def test_missing_wasm_and_cargo_failure_preserve_previous_output(self):
        self.previous_output()
        self.skip_artifact = "alpha-game"
        with self.assertRaises(FileNotFoundError):
            self.build()
        self.assert_previous_output()
        self.skip_artifact = None
        self.fail_package = "scene-game"
        with self.assertRaises(subprocess.CalledProcessError) as caught:
            self.build()
        self.assertEqual(caught.exception.returncode, 19)
        self.assert_previous_output()

    def test_source_changes_during_build_preserve_previous_output(self):
        self.previous_output()
        self.after_cargo = lambda _: self.write("web/runtime.js", "changed while building")
        with self.assertRaisesRegex(ValueError, "sources changed"):
            self.build()
        self.assert_previous_output()

    def test_nested_host_assets_are_never_excluded_from_source_provenance(self):
        path = self.write("web/alpha/assets/target/palette.bin", b"palette")
        self.build()
        info = self.manifest()
        relative = path.relative_to(self.root).as_posix()
        self.assertEqual(info["source"]["files"][relative],
                         info["files"]["alpha/assets/target/palette.bin"])
        self.previous_output()
        self.after_cargo = lambda _: path.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "sources changed"):
            self.build()
        self.assertEqual((self.output / "previous.txt").read_text(), "previous good output")

    def test_missing_host_or_shader_fails_before_cargo(self):
        for relative, error in (("web/audio-worklet.js", FileNotFoundError),
                                ("crates/ferravex-render/src/material.rs", ValueError)):
            with self.subTest(relative=relative):
                self.previous_output()
                path = self.root / relative
                data = path.read_bytes()
                if relative.endswith(".js"):
                    path.unlink()
                else:
                    path.write_text("// missing templates")
                self.commands.clear()
                with self.assertRaises(error):
                    self.build()
                self.assertFalse(any(command[0] == "cargo" for command, _ in self.commands))
                path.write_bytes(data)
                self.assert_previous_output()

    def test_collision_and_symlinks_are_rejected(self):
        self.previous_output()
        path = self.write("web/alpha/alpha_game.wasm", wasm())
        with self.assertRaisesRegex(ValueError, "collides"):
            self.build()
        path.unlink()
        for directory in (False, True):
            with self.subTest(directory=directory):
                path = self.root / "web/alpha/link"
                path.symlink_to(self.root / ("examples" if directory else "Cargo.toml"),
                                target_is_directory=directory)
                with self.assertRaisesRegex(ValueError, "symlink"):
                    self.build()
                path.unlink()
                self.assert_previous_output()

    def test_catalog_rejects_unsafe_names_duplicates_and_scene_paths(self):
        original = copy.deepcopy(self.games)
        cases = [
            ("host", "../escape"), ("host", "/absolute"), ("id", "--workspace"),
            ("host", "alpha"), ("id", "alpha-game"), ("id", "alpha_game"),
            ("editable_scene", "../scene.fxscene"), ("editable_scene", ""),
            ("editable_scene", "examples/scene-game/../map.fxscene"),
            ("editable_scene", "examples/alpha-game/map.fxscene"),
            ("editable_scene", "examples\\scene-game\\map.fxscene"),
            ("editable_scene", True),
        ]
        for key, value in cases:
            with self.subTest(key=key, value=value):
                self.games = copy.deepcopy(original)
                self.games[0][key] = value
                self.write_catalog()
                with self.assertRaises(ValueError):
                    builder.load_catalog(self.root)
        for invalid in ([], {}, [None]):
            self.write("examples/catalog.json", json.dumps(invalid))
            with self.assertRaises(ValueError):
                builder.load_catalog(self.root)

    def test_failed_publication_rolls_back_previous_tree(self):
        self.previous_output()
        rename = Path.rename

        def fail_publish(path, target):
            if path.name == "staged":
                raise OSError("fixture rename failure")
            return rename(path, target)

        with patch.object(Path, "rename", fail_publish), self.assertRaisesRegex(OSError, "rename failure"):
            self.build()
        self.assert_previous_output()

    def test_failed_rollback_keeps_recoverable_backup(self):
        self.previous_output()
        rename = Path.rename

        def fail_publish_and_rollback(path, target):
            if path.name == "staged" or path.name.endswith("-previous"):
                raise OSError("fixture rename failure")
            return rename(path, target)

        with patch.object(Path, "rename", fail_publish_and_rollback), \
                self.assertRaisesRegex(OSError, "previous output retained"):
            self.build()
        backups = list(self.output.parent.glob(".web-build-*-previous"))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / "previous.txt").read_text(), "previous good output")
        self.assertEqual((backups[0] / "build-info.json").read_text(), "previous manifest")

    @unittest.skipUnless(shutil.which("node"), "Node is unavailable")
    def test_node_reads_staged_wasm_exports_and_generated_shader_modules(self):
        self.build()
        program = r'''
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = process.argv[1];
const info = JSON.parse(fs.readFileSync(path.join(root, 'build-info.json')));
for (const game of Object.values(info.games)) {
  const module = new WebAssembly.Module(fs.readFileSync(path.join(root, game.wasm)));
  const exports = WebAssembly.Module.exports(module).filter(e => e.name.startsWith('ferravex_content_'));
  assert.equal(exports.length, game.variant === 'authoring' ? 10 : 0);
  assert.ok(exports.every(e => e.kind === 'function'));
}
(async () => {
  for (const [name, keys] of [
    ['retained-shader.js', ['vertexBody', 'fragmentBody', 'defaultVertexHook', 'defaultFragmentHook']],
    ['effects-shader.js', ['spriteVertexBody', 'spriteFragmentBody']],
  ]) {
    const data = fs.readFileSync(path.join(root, name));
    const module = await import('data:text/javascript;base64,' + data.toString('base64'));
    assert.deepEqual(Object.keys(module).sort(), keys.sort());
    for (const key of keys) assert.equal(module[key], key + '\\\n');
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
'''
        subprocess.run(["node", "-e", program, str(self.output)], check=True, capture_output=True, text=True)


class ProvenanceTests(BuildFixture):
    def provenance(self):
        with patch.object(builder, "_git", return_value=None):
            return builder.source_provenance(self.root)

    def test_content_hash_detects_edits_untracked_files_and_deletions(self):
        initial = self.provenance()
        self.assertIsNone(initial["revision"])
        self.assertIsNone(initial["dirty"])
        self.assertEqual(initial["sha256"], hashlib.sha256(builder._json_bytes(initial["files"])).hexdigest())
        path = self.write("examples/alpha-game/src/new.rs", "new untracked source")
        added = self.provenance()
        self.assertNotEqual(initial["sha256"], added["sha256"])
        path.write_text("changed untracked source")
        self.assertNotEqual(added["sha256"], self.provenance()["sha256"])
        path.unlink()
        self.assertEqual(initial, self.provenance())
        (self.root / "web/player.js").unlink()
        self.assertNotEqual(initial["sha256"], self.provenance()["sha256"])

    def test_cache_outputs_and_checkout_path_do_not_change_source_fingerprint(self):
        initial = self.provenance()
        for path in ("target/fixture.wasm", "dist/web/index.html", "scripts/__pycache__/build.pyc",
                     "examples/alpha-game/target/output", "scripts/ignored.pyc"):
            self.write(path, "generated")
        self.assertEqual(initial, self.provenance())
        clone = self.root.parent / "another checkout"
        shutil.copytree(self.root, clone)
        with patch.object(builder, "_git", return_value=None):
            self.assertEqual(initial, builder.source_provenance(clone))
        self.write(".cargo/config.toml", "[build]\nrustflags=[]\n")
        self.assertNotEqual(initial["sha256"], self.provenance()["sha256"])

    def test_no_git_executable_and_enclosing_checkout_are_not_claimed_clean(self):
        with patch.object(builder.subprocess, "run", side_effect=FileNotFoundError):
            info = builder.source_provenance(self.root)
        self.assertIsNone(info["dirty"])
        self.assertIsNone(info["revision"])
        with patch.object(builder, "_git", return_value=str(self.root.parent)):
            info = builder.source_provenance(self.root)
        self.assertIsNone(info["revision"])
        self.assertIsNone(info["dirty"])

    @unittest.skipUnless(shutil.which("git"), "Git is unavailable")
    def test_real_git_head_and_working_tree_state(self):
        def git(*args):
            return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True, text=True)
        git("init", "-q")
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture")
        clean = builder.source_provenance(self.root)
        self.assertEqual(clean["revision"], git("rev-parse", "HEAD").stdout.strip())
        self.assertFalse(clean["dirty"])
        self.write("web/player.js", "dirty tracked source")
        self.write("examples/alpha-game/src/untracked.rs", "untracked source")
        dirty = builder.source_provenance(self.root)
        self.assertTrue(dirty["dirty"])
        self.assertEqual(dirty["revision"], clean["revision"])
        self.assertNotEqual(dirty["sha256"], clean["sha256"])


class WasmAndCliTests(unittest.TestCase):
    def test_current_catalog_hosts_stage_in_a_temporary_directory(self):
        games = builder.load_catalog(ROOT)
        with tempfile.TemporaryDirectory(prefix="journey200-109-hosts-") as temporary:
            staged = Path(temporary)
            builder._stage_hosts(ROOT, staged, games)
            for game in games:
                host = ROOT / "web" / game["host"]
                for path in host.rglob("*"):
                    if path.is_file():
                        self.assertEqual((staged / game["host"] / path.relative_to(host)).read_bytes(),
                                         path.read_bytes())
            for name in builder.SHARED_ASSETS:
                self.assertEqual((staged / name).read_bytes(), (ROOT / "web" / name).read_bytes())
            self.assertEqual((staged / "games.json").read_bytes(), (ROOT / "examples/catalog.json").read_bytes())

    def test_malformed_export_sections_fail_with_clear_errors(self):
        with tempfile.TemporaryDirectory(prefix="journey200-109-wasm-") as temporary:
            path = Path(temporary) / "test.wasm"
            invalid = [b"bad", b"\0asm\x01\0\0\0\x07\x80",
                       b"\0asm\x01\0\0\0\x07\xff\xff\xff\xff\x7f",
                       b"\0asm\x01\0\0\0" + section(7, b"\x01\x7f"),
                       wasm() + section(7, b"\0"), wasm()[:-1]]
            for data in invalid:
                with self.subTest(data=data):
                    path.write_bytes(data)
                    with self.assertRaises(ValueError):
                        builder._check_variant(path, {"id": "fixture"})

    def test_import_has_no_subprocess_or_staging_side_effects(self):
        with patch.object(subprocess, "run", side_effect=AssertionError("import ran a command")), \
                patch.object(shutil, "rmtree", side_effect=AssertionError("import removed files")):
            load_script("build-owned.py")
            load_script("build-web.py")

    def test_wrapper_routes_profiles_and_propagates_failure(self):
        for args, profile in (([], "release"), (["release"], "release"), (["small"], "small")):
            with patch.object(web.subprocess, "run") as run:
                self.assertEqual(web.main(args), 0)
                run.assert_called_once_with(
                    [sys.executable, "-B", "scripts/build-owned.py", "--profile", profile],
                    cwd=web.ROOT, check=True,
                )
        with patch.object(web.subprocess, "run", side_effect=subprocess.CalledProcessError(23, "fixture")):
            self.assertEqual(web.main([]), 23)
        with patch.object(web.subprocess, "run") as run, contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as caught:
                web.main(["dev"])
            self.assertEqual(caught.exception.code, 2)
            run.assert_not_called()

    def test_owned_main_prints_report_and_clear_failure_without_traceback(self):
        with patch.object(builder, "build", return_value={"fixture": {"profile": "dev"}}) as build, \
                contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(builder.main(["--profile", "dev"]), 0)
        build.assert_called_once_with("dev")
        self.assertEqual(json.loads(output.getvalue()), {"fixture": {"profile": "dev"}})
        for error, code in ((ValueError("fixture error"), 1),
                            (FileNotFoundError("missing artifact"), 1),
                            (subprocess.CalledProcessError(37, "cargo"), 37)):
            with patch.object(builder, "build", side_effect=error), \
                    contextlib.redirect_stderr(io.StringIO()) as output:
                self.assertEqual(builder.main([]), code)
            self.assertIn("web build failed:", output.getvalue())
            self.assertNotIn("Traceback", output.getvalue())


if __name__ == "__main__":
    unittest.main()
