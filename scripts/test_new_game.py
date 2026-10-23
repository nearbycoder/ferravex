#!/usr/bin/env python3
"""Scaffold regressions using only Python stdlib; no compiler/display required."""
import contextlib
from html.parser import HTMLParser
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent


def load_script(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


generator = load_script(ROOT / "scripts/new-game.py", "new_game")


class ScaffoldFixture:
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="ferravex-scaffold-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()

    def generate(self, name="moon-garden", template="2d", destination=None, **options):
        return generator.generate(name, destination or self.root / name, template, **options)


class GenerationTests(ScaffoldFixture, unittest.TestCase):
    def test_both_templates_are_isolated_facade_projects(self):
        for template in ("2d", "3d"):
            with self.subTest(template=template):
                name = "moon-" + template
                output = self.generate(name, template)
                manifest = tomllib.loads((output / "Cargo.toml").read_text(encoding="utf-8"))
                self.assertEqual(manifest["package"]["name"], name)
                self.assertEqual(manifest["package"]["edition"], "2024")
                self.assertIn("workspace", manifest)
                self.assertEqual(set(manifest["dependencies"]), {"ferravex"})
                self.assertEqual(Path(manifest["dependencies"]["ferravex"]["path"]), ROOT / "crates/ferravex")
                self.assertEqual(manifest["lib"]["crate-type"], ["rlib", "cdylib"])
                self.assertIn(name.replace("-", "_") + "::run()", (output / "src/main.rs").read_text(encoding="utf-8"))
                self.assertIn("ferravex::render::game!", (output / "src/lib.rs").read_text(encoding="utf-8"))
                rules = (output / "src/game.rs").read_text(encoding="utf-8")
                self.assertIn("pub const TICK_RATE_HZ: f64 = 60.0;", rules)
                self.assertIn("GameSession::new(Self::TICK_RATE_HZ, 8, autoplay)", rules)
                self.assertIn("Camera" + template.upper(), (output / "src/lib.rs").read_text(encoding="utf-8"))
                for path in output.rglob("*"):
                    if path.is_file():
                        self.assertIsNone(generator.TOKEN.search(path.read_text(encoding="utf-8")), str(path))
                        self.assertNotEqual(path.suffix, ".in")
                for path in ("README.md", "AGENTS.md", ".gitignore", "ferravex.json", "scripts/build-web.py", "web/index.html"):
                    self.assertTrue((output / path).is_file(), path)

    def test_lock_is_exact_first_party_dependency_closure(self):
        output = self.generate()
        packages = tomllib.loads((output / "Cargo.lock").read_text(encoding="utf-8"))["package"]
        by_name = {p["name"]: p for p in packages}
        reached = set()

        def visit(name):
            if name in reached:
                return
            reached.add(name)
            self.assertNotIn("source", by_name[name])
            self.assertNotIn("checksum", by_name[name])
            for dependency in by_name[name].get("dependencies", []):
                visit(dependency)

        visit("moon-garden")
        self.assertEqual(reached, set(by_name))
        self.assertEqual(by_name["moon-garden"]["dependencies"], ["ferravex"])
        self.assertNotIn("ferravex-bench", by_name)

    def test_paths_with_spaces_quotes_backslashes_unicode_and_tokens_round_trip(self):
        suffix = ' space "quote" \\ slash' if os.name != "nt" else " space 'quote'"
        engine = self.root / ("engine" + suffix + " café @@NAME@@")
        (engine / "crates/ferravex").mkdir(parents=True)
        shutil.copyfile(ROOT / "crates/ferravex/Cargo.toml", engine / "crates/ferravex/Cargo.toml")
        shutil.copyfile(ROOT / "Cargo.lock", engine / "Cargo.lock")
        shutil.copytree(ROOT / "templates", engine / "templates")
        before = {p.relative_to(engine): p.read_bytes() for p in engine.rglob("*") if p.is_file()}
        destination = self.root / ("game" + suffix + " café $(do-not-run)")
        with patch.object(subprocess, "run", side_effect=AssertionError("generator must not run commands")):
            output = generator.generate("star_field", destination, "3d", engine_root=engine)
        manifest = tomllib.loads((output / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertEqual(manifest["dependencies"]["ferravex"]["path"], (engine / "crates/ferravex").as_posix())
        self.assertEqual(json.loads((output / "ferravex.json").read_text(encoding="utf-8"))["engine_root"], engine.as_posix())
        after = {p.relative_to(engine): p.read_bytes() for p in engine.rglob("*") if p.is_file()}
        self.assertEqual(before, after)

    def test_existing_destinations_are_never_overwritten(self):
        empty = self.root / "empty"
        empty.mkdir()
        occupied = self.root / "occupied"
        occupied.mkdir()
        (occupied / "keep.txt").write_text("keep me")
        file = self.root / "file"
        file.write_text("keep file")
        link = self.root / "link"
        link.symlink_to(occupied, target_is_directory=True)
        dangling = self.root / "dangling"
        dangling.symlink_to(self.root / "absent")
        for destination in (empty, occupied, file, link, dangling):
            with self.subTest(destination=destination), self.assertRaisesRegex(ValueError, "overwrite"):
                self.generate(destination=destination)
        self.assertEqual(list(empty.iterdir()), [])
        self.assertEqual((occupied / "keep.txt").read_text(encoding="utf-8"), "keep me")
        self.assertEqual(file.read_text(encoding="utf-8"), "keep file")
        self.assertTrue(link.is_symlink())
        self.assertTrue(dangling.is_symlink())

    def test_invalid_names_and_destinations_leave_no_project(self):
        destination = self.root / "game"
        for name in ("", "../escape", "has space", "bad/name", "two\nlines", "1first", "Upper", "under__score", "trailing-", "a" * 65, "async", "gen", "self", "test", "con", "com1", "ferravex", "ferravex-core"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.generate(name=name, destination=destination)
            self.assertFalse(destination.exists())
        for invalid in (self.root / "missing" / "child", self.root / "new\nline", self.root / "absent" / ".."):
            with self.subTest(destination=invalid), self.assertRaises(ValueError):
                self.generate(destination=invalid)
        self.assertFalse((self.root / "missing").exists())
        with self.assertRaises(ValueError):
            self.generate(template="4d", destination=destination)

    def test_write_failure_cleans_only_our_new_directory(self):
        destination = self.root / "game"
        sentinel = self.root / "keep.txt"
        sentinel.write_text("keep")
        original_open = Path.open

        def failing_open(path, *args, **kwargs):
            if path == destination / "README.md" and args and args[0] == "x":
                raise OSError("simulated disk failure")
            return original_open(path, *args, **kwargs)

        with patch.object(Path, "open", failing_open), self.assertRaisesRegex(OSError, "disk failure"):
            self.generate(destination=destination)
        self.assertFalse(destination.exists())
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_cli_default_destination_and_failure_exit_codes(self):
        command = [sys.executable, "-B", str(ROOT / "scripts/new-game.py"), "cli-game"]
        result = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.root / "cli-game/Cargo.toml").is_file())
        result = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("refusing to overwrite", result.stderr)
        for args in (["new-name", ""], ["bad/name"], ["valid", "--template", "4d"]):
            result = subprocess.run(command[:-1] + args, cwd=self.root, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "new-name").exists())

    def test_cli_quotes_printed_shell_path_without_executing_it(self):
        destination = self.root / "space 'quote' $(printf nope)"
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            self.assertEqual(generator.main(["shell-path", str(destination)]), 0)
        import shlex
        cd_line = next(line.strip() for line in stdout.getvalue().splitlines() if line.strip().startswith("cd "))
        self.assertEqual(shlex.split(cd_line), ["cd", str(destination)])

    def test_common_template_files_stay_in_sync(self):
        for path in (ROOT / "templates/game-2d").rglob("*.in"):
            relative = path.relative_to(ROOT / "templates/game-2d")
            if relative == Path("src/lib.rs.in"):
                continue
            self.assertEqual(path.read_bytes(), (ROOT / "templates/game-3d" / relative).read_bytes(), str(relative))


class CustomizationTests(ScaffoldFixture, unittest.TestCase):
    def test_manifest_preserves_helper_fields_and_lists_all_generated_files(self):
        for template in ("2d", "3d"):
            with self.subTest(template=template):
                name = "manifest-" + template
                output = self.generate(name, template)
                manifest = json.loads((output / "ferravex.json").read_text(encoding="utf-8"))
                self.assertEqual(manifest["schema_version"], 1)
                self.assertEqual(manifest["engine_root"], ROOT.as_posix())
                self.assertEqual(manifest["package"], name)
                self.assertEqual(manifest["template"], template.upper())
                self.assertEqual(manifest["tick_rate"], 60.0)
                self.assertEqual(manifest["window_title"], name)
                files = sorted(p.relative_to(output).as_posix() for p in output.rglob("*") if p.is_file())
                self.assertEqual(manifest["generated_files"], files)
                for path in (".gitignore", "Cargo.lock", "ferravex.json"):
                    self.assertIn(path, manifest["generated_files"])
                # Generation metadata does not depend on the output path/time.
                second = self.generate(name, template, destination=self.root / (name + "-again"))
                for path in files:
                    self.assertEqual((output / path).read_bytes(), (second / path).read_bytes(), path)

    def test_tick_rate_boundaries_and_fractional_rates_reach_rules_and_manifest(self):
        for template in ("2d", "3d"):
            for rate in (1, 30, 59.94, 120.5, 1000):
                with self.subTest(template=template, rate=rate):
                    name = f"rate-{template}-{str(rate).replace('.', '-')}"
                    output = self.generate(name, template, tick_rate=rate)
                    manifest = json.loads((output / "ferravex.json").read_text(encoding="utf-8"))
                    self.assertEqual(manifest["tick_rate"], rate)
                    rules = (output / "src/game.rs").read_text(encoding="utf-8")
                    self.assertIn(f"pub const TICK_RATE_HZ: f64 = {float(rate)!r};", rules)
                    self.assertIn("let frame = game.advance(1.0 / Game::TICK_RATE_HZ, Vec2::X);", rules)
                    self.assertIn(f"generated rate is {float(rate)!r} Hz", (output / "README.md").read_text(encoding="utf-8"))

    def test_titles_are_literal_data_in_rust_json_and_html_without_changing_ids(self):
        class Page(HTMLParser):
            def __init__(self):
                super().__init__()
                self.elements = []
                self.title = ""
                self.in_title = False

            def handle_starttag(self, tag, attrs):
                self.elements.append((tag, dict(attrs)))
                if tag == "title":
                    self.in_title = True

            def handle_endtag(self, tag):
                if tag == "title":
                    self.in_title = False

            def handle_data(self, data):
                if self.in_title:
                    self.title += data

        title = 'Lune "🌙" \\ </title><script>alert(1)</script> & \'café\' @@NAME@@'
        for template in ("2d", "3d"):
            with self.subTest(template=template):
                name = "title-" + template
                output = self.generate(name, template, window_title=title)
                manifest = json.loads((output / "ferravex.json").read_text(encoding="utf-8"))
                self.assertEqual(manifest["window_title"], title)
                game = tomllib.loads((output / "Cargo.toml").read_text(encoding="utf-8"))
                self.assertEqual(game["package"]["name"], name)
                library = (output / "src/lib.rs").read_text(encoding="utf-8")
                literal = re.search(r"render::window_conf\((.*)\)", library)[1]
                self.assertEqual(json.loads(literal), title)
                self.assertEqual(library.count(literal), 2)  # Window and in-game header.
                self.assertIn(f'PlayerInput::preset("{name}", Preset::Explore)', library)
                guide = json.loads((output / "web/games.json").read_text(encoding="utf-8"))
                self.assertEqual(guide[0]["id"], name)
                self.assertEqual(guide[0]["title"], title)
                page = Page()
                page.feed((output / "web/index.html").read_text(encoding="utf-8"))
                self.assertEqual(page.title, title + " · Ferravex")
                self.assertEqual([attrs for tag, attrs in page.elements if tag == "script"],
                                 [{"type": "module", "src": "./runtime.js"}])
                canvas = next(attrs for tag, attrs in page.elements if tag == "canvas")
                self.assertEqual(canvas["data-game"], name)
                self.assertEqual(canvas["aria-label"], title + " game: WASD or arrows to move, Enter to start, Escape to pause")
                self.assertEqual(set(canvas), {"tabindex", "data-game", "aria-label"})

    def test_invalid_options_fail_before_creating_any_project_directory(self):
        rates = (0, -1, 0.99, 1000.01, float("nan"), float("inf"), -float("inf"),
                 True, False, None, "60", [], 10 ** 10000)
        titles = ("", "   ", "x" * 121, "line\nbreak", "tab\there", "null\0here",
                  "\x1bterminal", "\x7f", "\x85", "\u2028", "\u202e", "\ud800", 60, True, b"title", [])
        for option, values in (("tick_rate", rates), ("window_title", titles)):
            for index, value in enumerate(values):
                with self.subTest(option=option, index=index):
                    with patch.object(Path, "mkdir", side_effect=AssertionError("must validate before mkdir")):
                        with self.assertRaisesRegex(ValueError, "tick rate|window title"):
                            self.generate(**{option: value})
                    self.assertEqual(list(self.root.iterdir()), [])

    def test_title_length_boundary_preserves_unicode_and_spaces(self):
        title = " " + "🌙" * 118 + " "
        output = self.generate(window_title=title)
        self.assertEqual(json.loads((output / "ferravex.json").read_text(encoding="utf-8"))["window_title"], title)

    def test_cli_customization_and_invalid_option_errors(self):
        command = [sys.executable, "-B", str(ROOT / "scripts/new-game.py")]
        title = 'CLI "Moon" & stars'
        result = subprocess.run(command + ["cli-options", "--template", "3d", "--tick-rate", "59.94",
                                           "--window-title", title], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((self.root / "cli-options/ferravex.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["tick_rate"], 59.94)
        self.assertEqual(manifest["window_title"], title)
        self.assertEqual(manifest["template"], "3D")
        for args, message in ((["--tick-rate=nan"], "tick rate"),
                              (["--tick-rate=inf"], "tick rate"),
                              (["--tick-rate=-inf"], "tick rate"),
                              (["--tick-rate=0"], "tick rate"),
                              (["--tick-rate=1001"], "tick rate"),
                              (["--window-title", ""], "window title"),
                              (["--window-title", "bad\nline"], "window title")):
            with self.subTest(args=args):
                result = subprocess.run(command + ["invalid-options"] + args, cwd=self.root, capture_output=True, text=True)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(message, result.stderr)
                self.assertNotIn("Traceback", result.stderr)
                self.assertFalse((self.root / "invalid-options").exists())

    def test_racing_destination_is_preserved_with_custom_options(self):
        destination = self.root / "racing-game"
        original_mkdir = Path.mkdir

        def raced_mkdir(path, *args, **kwargs):
            if path == destination:
                original_mkdir(path)
                (path / "keep.txt").write_text("created by another caller")
                raise FileExistsError("simulated concurrent generator")
            return original_mkdir(path, *args, **kwargs)

        with patch.object(Path, "mkdir", raced_mkdir), self.assertRaises(FileExistsError):
            self.generate(destination=destination, tick_rate=120, window_title="Racing game")
        self.assertEqual((destination / "keep.txt").read_text(encoding="utf-8"), "created by another caller")
        self.assertEqual(len(list(destination.iterdir())), 1)

    def test_manifest_write_failure_removes_partial_generation(self):
        destination = self.root / "failed-manifest"
        original_open = Path.open

        def failing_open(path, *args, **kwargs):
            if path == destination / "ferravex.json" and args and args[0] == "x":
                raise OSError("manifest write failed")
            return original_open(path, *args, **kwargs)

        with patch.object(Path, "open", failing_open), self.assertRaisesRegex(OSError, "manifest write failed"):
            self.generate(destination=destination, tick_rate=120, window_title="Failure")
        self.assertFalse(destination.exists())

    def test_incomplete_manifest_template_is_rejected_before_writing(self):
        engine = self.root / "incomplete-engine"
        (engine / "crates/ferravex").mkdir(parents=True)
        shutil.copyfile(ROOT / "crates/ferravex/Cargo.toml", engine / "crates/ferravex/Cargo.toml")
        shutil.copyfile(ROOT / "Cargo.lock", engine / "Cargo.lock")
        shutil.copytree(ROOT / "templates/game-2d", engine / "templates/game-2d")
        (engine / "templates/game-2d/ferravex.json.in").unlink()
        destination = self.root / "incomplete-game"
        with self.assertRaisesRegex(ValueError, "incomplete template"):
            self.generate(destination=destination, engine_root=engine)
        self.assertFalse(destination.exists())


class WebBuildTests(ScaffoldFixture, unittest.TestCase):
    def test_web_build_routes_quoted_paths_and_stages_complete_host(self):
        output = self.generate(destination=self.root / "a 'quoted' game", template="3d",
                               tick_rate=120, window_title='Moon "Garden" & stars')
        helper = load_script(output / "scripts/build-web.py", "generated_web_build")
        wasm = output / "target/wasm32-unknown-unknown/debug/moon_garden.wasm"
        wasm.parent.mkdir(parents=True)
        wasm.write_bytes(b"\0asm\x01\0\0\0")
        with patch.object(helper.subprocess, "run") as run, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(helper.main(["--profile", "dev"]), 0)
        run.assert_called_once()
        args, kwargs = run.call_args
        command = args[0]
        self.assertEqual(command[command.index("--manifest-path") + 1], str(output / "Cargo.toml"))
        self.assertEqual(command[command.index("--target-dir") + 1], str(output / "target"))
        self.assertEqual(command[command.index("--jobs") + 1], "2")
        self.assertIn("--offline", command)
        self.assertIn("--locked", command)
        self.assertTrue(kwargs["check"])
        self.assertEqual(kwargs["cwd"], output)
        self.assertNotIn("shell", kwargs)
        staged = output / "target/web"
        self.assertEqual((staged / wasm.name).read_bytes(), wasm.read_bytes())
        self.assertIn('data-game="moon-garden"', (staged / "index.html").read_text(encoding="utf-8"))
        guide = json.loads((staged / "games.json").read_text(encoding="utf-8"))
        self.assertEqual(guide[0]["id"], "moon-garden")
        self.assertEqual(guide[0]["title"], 'Moon "Garden" & stars')
        self.assertIn("3D", guide[0]["description"])
        self.assertIn("Escape", guide[0]["controls"])
        for path in staged.glob("*.js"):
            for reference in re.findall(r"['\"](\./[^'\"]+\.js)['\"]", path.read_text(encoding="utf-8")):
                self.assertTrue((path.parent / reference).is_file(), f"{path.name}: {reference}")
        for name in ("retained-shader.js", "effects-shader.js", "audio-worklet.js", "runtime.js", "input.js"):
            self.assertTrue((staged / name).is_file(), name)

    def test_web_build_propagates_cargo_failure_without_staging(self):
        output = self.generate()
        helper = load_script(output / "scripts/build-web.py", "failed_web_build")
        with patch.object(helper.subprocess, "run", side_effect=subprocess.CalledProcessError(37, ["cargo"])):
            self.assertEqual(helper.main([]), 37)
        self.assertFalse((output / "target/web").exists())


if __name__ == "__main__":
    unittest.main()
