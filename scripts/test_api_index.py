#!/usr/bin/env python3
"""Isolated stdlib tests for api-index.py; no Cargo, site rebuild or network.

Fixtures use the inspected local Rustdoc layout (rustdoc 1.96.1). The optional
read-only evidence test checks an existing generated page/source pair without
claiming that those old artifacts are fresh. Every output is in a temporary dir.
"""

from contextlib import redirect_stdout
import html
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

SCRIPT = Path(__file__).with_name("api-index.py")
SPEC = importlib.util.spec_from_file_location("api_index", SCRIPT)
api = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(api)

SOURCE = "//! Fixture crate.\npub struct Rig3;\nimpl Rig3 { pub fn update(&mut self, dt: f32) {} }\n"


def source_html(path, source):
    numbered = "".join(f'<a href="#{i}" id="{i}" data-nosnippet>{i}</a>{html.escape(line)}'
                       for i, line in enumerate(source.splitlines(keepends=True), 1))
    return (f'<meta name="generator" content="rustdoc"><meta name="description" '
            f'content="Source of the Rust file `{path}`."><pre class="rust"><code>{numbered}</code></pre>')


def document(title, heading, signature="", members="", source="../src/demo/lib.rs.html#1-3", extra=""):
    return (f'<html><head><meta name="generator" content="rustdoc">'
            f'<meta name="rustdoc-vars" data-rustdoc-version="fixture-1.96.1">'
            f'<title>{html.escape(title)} - Rust</title><script>invented_symbol</script></head>'
            f'<body><nav>fake_navigation</nav><main><section id="main-content">'
            f'<div class="main-heading"><h1>{heading}<button>Copy item path</button></h1>'
            f'<a class="src" href="{source}">Source</a></div>'
            + (f'<pre class="rust item-decl"><code>{signature}</code></pre>' if signature else "")
            + extra + members + '</section></main></body></html>')


def member(name="update", anchor_kind="method", signature=None, source="../src/demo/lib.rs.html#3", wrapper=True):
    if signature is None:
        signature = f'pub fn <a class="fn" href="#{anchor_kind}.{html.escape(name)}">{html.escape(name)}</a>(&amp;mut self, dt: f32)'
    section = (f'<section id="{anchor_kind}.{html.escape(name)}" class="method">'
               f'<a class="src" href="{source}">Source</a><h4 class="code-header">'
               f'{signature}</h4></section>')
    return f'<details class="toggle method-toggle"><summary>{section}</summary><div class="docblock">Prose must not leak</div></details>' if wrapper else section


class IndexTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ferravex api tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/demo"]\n')
        self.write("Cargo.lock", "version = 4\n")
        self.write("crates/demo/Cargo.toml", '[package]\nname = "demo"\nversion = "0.1.0"\n[features]\nextra = []\n')
        self.write("crates/demo/src/lib.rs", SOURCE)
        (self.root / "target").mkdir()
        self.snapshot = api.prepare(self.root, packages=["demo"], features=["extra"], no_default_features=True)
        self.write("target/doc/src/demo/lib.rs.html", source_html("crates/demo/src/lib.rs", SOURCE))
        self.page("index.html", document("demo", "Crate demo"))
        self.page("misleading.filename.html", document("Rig3 in demo", "Struct Rig<wbr>3",
                  "pub struct Rig3;", '<div id="implementations-list">' + member() + '</div>'))
        self.page("fn.cast_ball.html", document("cast_ball in demo", "Function cast_ball", "pub fn cast_ball()"))
        self.page("fn.cast_balloon.html", document("cast_balloon in demo", "Function cast_balloon", "pub fn cast_balloon()"))
        self.page("redirect.html", '<meta http-equiv="refresh" content="0;URL=misleading.filename.html">')

    def write(self, relative, text):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8", newline="\n")
        if relative.startswith("target/doc/") and hasattr(self, "snapshot"):
            timestamp = max(path.stat().st_mtime_ns, self.snapshot["prepared_ns"] + 1)
            os.utime(path, ns=(timestamp, timestamp))
        return path

    def page(self, name, text):
        return self.write("target/doc/demo/" + name, text)

    def build(self):
        return api.build_index(self.root, "target/doc", self.snapshot)

    def save_index(self, name="target/index.json"):
        index = self.build()
        self.write(name, api._json_bytes(index).decode())
        return index

    def search(self, query, **kwargs):
        return api.search_index(self.root, "target/index.json", query, **kwargs)

    def invoke(self, *args):
        output = io.StringIO()
        with redirect_stdout(output):
            status = api.main(["--root", str(self.root), *args])
        return status, json.loads(output.getvalue())

    def assert_code(self, code, function, *args, **kwargs):
        with self.assertRaises(api.IndexError) as raised:
            function(*args, **kwargs)
        self.assertEqual(raised.exception.code, code, str(raised.exception))

    def test_actual_paths_signatures_sources_and_declared_features(self):
        index = self.save_index()
        rows = {s["qualified_path"]: s for s in index["symbols"]}
        self.assertEqual(set(rows), {"demo", "demo::Rig3", "demo::Rig3::update", "demo::cast_ball", "demo::cast_balloon"})
        self.assertEqual(index["skipped_pages"], 1)
        self.assertEqual(rows["demo"]["signature"], None)
        method = rows["demo::Rig3::update"]
        self.assertEqual(method["signature"], "pub fn update(&mut self, dt: f32)")
        self.assertEqual(method["import_path"], "demo::Rig3")
        self.assertEqual(method["kind"], "method")
        self.assertEqual(method["source"]["path"], "crates/demo/src/lib.rs")
        self.assertEqual(method["source"]["line_start"], 3)
        self.assertEqual(method["features"]["configuration"], "snapshot.configuration")
        self.assertFalse(method["features"]["requirements_known"])
        self.assertIsNone(method["features"]["availability"])
        result = self.search("Rig3::update")["results"][0]
        self.assertEqual(result["features"]["configuration"]["features"], ["extra"])
        self.assertTrue(result["features"]["configuration"]["no_default_features"])
        self.assertIn("misleading.filename.html#method.update", result["doc"]["url"])
        self.assertIn("%20", result["doc"]["url"])
        self.assertNotIn("Source", result["signature"])
        self.assertEqual(result["provenance"]["page_sha256"], api._sha((self.root / result["doc"]["path"]).read_bytes()))

    def test_exact_auto_fuzzy_prefix_suffix_ranking_and_empty_results(self):
        self.save_index()
        self.assertEqual(self.search("cast_ball")["total_matches"], 1)
        fuzzy = self.search("cast_ball", mode="fuzzy")
        self.assertEqual([s["name"] for s in fuzzy["results"]], ["cast_ball", "cast_balloon"])
        self.assertEqual([s["match"] for s in fuzzy["results"]], ["exact", "prefix"])
        self.assertEqual(self.search("cast_bal")["results"][0]["name"], "cast_ball")
        self.assertEqual(self.search("Rig4")["results"][0]["qualified_path"], "demo::Rig3")
        self.assertEqual(self.search("Rig4", mode="exact")["results"], [])
        self.assertEqual(self.search("not_a_symbol_here")["results"], [])
        self.assertEqual(self.search("`::DEMO :: Rig3 :: update()`")["results"][0]["name"], "update")
        self.assertEqual(self.search("update", mode="exact")["results"][0]["name"], "update")
        self.assertEqual(self.search("fake_navigation")["total_matches"], 0)
        self.assertEqual(self.search("misleading")["total_matches"], 0)

    def test_deterministic_bytes_and_bounded_results(self):
        first = self.save_index()
        self.assertEqual(api._json_bytes(first), api._json_bytes(self.build()))
        before = self.search("cast", mode="fuzzy", limit=1)
        after = self.search("cast", mode="fuzzy", limit=1)
        self.assertEqual(api._json_bytes(before), api._json_bytes(after))
        self.assertEqual(before["total_matches"], 2)
        self.assertTrue(before["truncated"])
        self.assertEqual(len(before["results"]), 1)

    def test_reexport_identity_comes_from_rustdoc_title(self):
        self.page("unrelated.html", document("Alias in demo", "Struct Alias", "pub struct Alias;"))
        rows = {s["qualified_path"]: s for s in self.build()["symbols"]}
        self.assertEqual(rows["demo::Alias"]["import_path"], "demo::Alias")
        self.assertEqual(rows["demo::Alias"]["source"]["path"], "crates/demo/src/lib.rs")

    def test_traits_associated_items_raw_identifiers_unicode_and_macros(self):
        items = member("r#type") + member("step", "tymethod", wrapper=False)
        items += member("LIMIT", "associatedconstant", 'const <a href="#associatedconstant.LIMIT">LIMIT</a>: usize = 4')
        items += member("Output", "associatedtype", 'type <a href="#associatedtype.Output">Output</a>')
        self.page("trait.Action.html", document("Action in demo", "Trait Action", "pub trait Action {}", items))
        self.page("macro.game.html", document("game in demo", "Macro game", "macro_rules! game { () =&gt; {...} }"))
        self.page("fn.Élan.html", document("Élan in demo", "Function Élan", "pub fn Élan()"))
        self.save_index()
        self.assertEqual(self.search("Action::type")["results"][0]["qualified_path"], "demo::Action::r#type")
        self.assertEqual(self.search("LIMIT")["results"][0]["kind"], "associated_constant")
        self.assertEqual(self.search("Output")["results"][0]["kind"], "associated_type")
        self.assertEqual(self.search("step")["results"][0]["kind"], "required_method")
        self.assertEqual(self.search("game!")["results"][0]["kind"], "macro")
        self.assertEqual(self.search("élan")["results"][0]["name"], "Élan")

    def test_exclude_external_trait_blanket_deref_and_missing_heading_evidence(self):
        body = ''.join(f'<div id="{ident}">{member("phantom_" + str(i))}</div>' for i, ident in enumerate(
            ("trait-implementations-list", "blanket-implementations-list", "synthetic-implementations-list", "deref-methods")))
        self.page("struct.Empty.html", document("Empty in demo", "Struct Empty", "pub struct Empty;", body))
        self.page("fn.invented.html", '<title>invented in demo - Rust</title><main id="main-content">pub fn invented()</main>')
        self.page("struct.External.html", document("External in demo", "Struct External", "pub struct External;", source="https://example.com/src.rs.html"))
        self.save_index()
        self.assertEqual(self.search("phantom")["total_matches"], 0)
        self.assertEqual(self.search("invented", mode="exact")["total_matches"], 0)
        self.assertEqual(self.search("External", mode="exact")["total_matches"], 0)

    def test_trait_foreign_implementations_and_docblock_markup_are_not_declarations(self):
        foreign = member("step", signature='fn <a href="#tymethod.step">step</a>()')
        members = member("step", "tymethod", wrapper=False)
        members += '<h2 id="foreign-impls">Implementations on foreign types</h2>'
        members += '<details class="toggle implementors-toggle"><summary>impl Action for u8</summary><div class="impl-items">' + foreign + '</div></details>'
        members += '<div class="docblock"><div class="methods">' + member("invented") + '</div><pre class="rust item-decl">invented signature</pre></div>'
        self.page("trait.Action.html", document("Action in demo", "Trait Action", "pub trait Action {}", members))
        self.page("fn.linebreak.html", document("linebreak in demo", "Function linebreak", "pub fn linebreak()<br>where T: Copy"))
        rows = {s["qualified_path"]: s for s in self.build()["symbols"]}
        self.assertEqual(rows["demo::Action"]["signature"], "pub trait Action {}")
        self.assertEqual(rows["demo::Action::step"]["kind"], "required_method")
        self.assertNotIn("demo::Action::invented", rows)
        self.assertEqual(rows["demo::linebreak"]["signature"], "pub fn linebreak() where T: Copy")

    def test_signature_and_availability_clipping_is_explicit(self):
        self.page("fn.long.html", document("long in demo", "Function long", "pub fn long(" + "a" * 1000 + ")",
                  extra='<span class="item-info"><div class="stab portability">Available with ' + 'x' * 500 + '</div></span>'))
        row = next(s for s in self.build()["symbols"] if s["name"] == "long")
        self.assertEqual(len(row["signature"]), api.MAX_SIGNATURE)
        self.assertTrue(row["signature_truncated"])
        self.assertTrue(row["features"]["availability_truncated"])
        self.assertEqual(len(row["features"]["availability"]), api.MAX_AVAILABILITY)

    def test_current_source_change_addition_deletion_and_reset(self):
        self.save_index()
        source = self.root / "crates/demo/src/lib.rs"
        source.write_text(SOURCE + "// change\n")
        self.assert_code("stale_index", self.search, "Rig3")
        source.write_text(SOURCE)
        self.assertEqual(self.search("Rig3")["total_matches"], 1)
        added = self.write("crates/demo/src/new.rs", "// not registered; still a build input change\n")
        self.assert_code("stale_index", self.search, "Rig3")
        added.unlink()
        self.assertEqual(self.search("Rig3")["total_matches"], 1)
        source.unlink()
        self.assert_code("stale_index", self.search, "Rig3")

    def test_manifest_lock_config_and_untracked_documentation_changes_stale(self):
        self.save_index()
        for relative in ("Cargo.lock", "crates/demo/Cargo.toml", ".cargo/config.toml", "docs/new.md"):
            with self.subTest(relative=relative):
                path = self.root / relative
                previous = path.read_text() if path.exists() else None
                self.write(relative, (previous or "") + "\n# added\n")
                self.assert_code("stale_index", self.search, "Rig3")
                if previous is None:
                    path.unlink()
                else:
                    path.write_text(previous)
        self.assertEqual(self.search("Rig3")["total_matches"], 1)

    def test_old_artifacts_and_embedded_old_source_are_rejected(self):
        path = self.root / "target/doc/demo/index.html"
        os.utime(path, ns=(1, 1))
        self.assert_code("stale_index", self.build)
        os.utime(path, ns=(self.snapshot["prepared_ns"] + 1,) * 2)
        self.write("target/doc/src/demo/lib.rs.html", source_html("crates/demo/src/lib.rs", SOURCE + "// stale\n"))
        self.assert_code("stale_index", self.build)

    def test_artifact_mutation_addition_deletion_and_cache_reset(self):
        self.save_index()
        path = self.root / "target/doc/demo/index.html"
        old = path.read_text()
        path.write_text(old + "<!-- changed -->")
        self.assert_code("stale_index", self.search, "Rig3")
        path.write_text(old)
        self.assertEqual(self.search("Rig3")["total_matches"], 1)
        added = self.page("unknown.html", "unrecognized")
        self.assert_code("stale_index", self.search, "Rig3")
        added.unlink()
        path.unlink()
        self.assert_code("missing_index", self.search, "Rig3")

    def test_missing_tampered_malformed_and_unsupported_indexes(self):
        self.assert_code("missing_index", self.search, "Rig3")
        index = self.save_index()
        index["symbols"][0]["name"] = "invented"
        self.write("target/index.json", json.dumps(index))
        self.assert_code("invalid_index", self.search, "Rig3")
        for value in ("{", "[]", '{"schema_version":99}', 'null'):
            self.write("target/index.json", value)
            self.assert_code("invalid_index", self.search, "Rig3")

    def test_checksum_correct_malformed_records_and_oversized_fields_fail(self):
        original = self.save_index()
        mutations = [
            lambda data: data.update(symbols=[None]),
            lambda data: data["symbols"][0].update(signature="x" * (api.MAX_SIGNATURE + 1)),
            lambda data: data["symbols"][0].update(import_path="invented::path"),
            lambda data: data["symbols"][0].update(extra="unexpected"),
            lambda data: data["symbols"][0]["provenance"].update(page_sha256="0" * 64),
            lambda data: data.update(artifacts=[]),
            lambda data: data.update(rustdoc_versions=["x" * (api.MAX_PATH + 1)]),
        ]
        for mutate in mutations:
            data = json.loads(json.dumps(original))
            mutate(data)
            data.pop("payload_sha256")
            self.write("target/index.json", api._json_bytes(api._sealed(data)).decode())
            self.assert_code("invalid_index", self.search, "Rig3")

    def test_library_manifest_names_and_binary_packages(self):
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/demo", "crates/tool"]\n')
        self.write("crates/tool/Cargo.toml", '[package]\nname = "tool"\nversion = "0.1.0"\n')
        self.write("crates/tool/src/main.rs", "fn main() {}\n")
        self.assertEqual([p["package"] for p in api.prepare(self.root)["packages"]], ["demo"])
        self.assert_code("invalid_workspace", api.prepare, self.root, packages=["tool"])
        self.write("crates/demo/Cargo.toml", '[package]\nname = "some-name"\nversion = "0.1.0"\n[lib]\nname = "actual_crate"\n')
        self.assertEqual(api.prepare(self.root, packages=["some-name"])["packages"][0]["crate"], "actual_crate")

    def test_snapshot_schema_and_source_line_errors(self):
        snapshot = dict(self.snapshot, packages=[None])
        snapshot.pop("payload_sha256")
        self.assert_code("invalid_index", api.build_index, self.root, "target/doc", api._sealed(snapshot))
        self.page("fn.bad_line.html", document("bad_line in demo", "Function bad_line", "pub fn bad_line()",
                                              source="../src/demo/lib.rs.html#3-999"))
        self.assert_code("invalid_evidence", self.build)

    def test_no_symbols_and_conflicting_html_evidence_fail_closed(self):
        page = self.root / "target/doc/demo/misleading.filename.html"
        text = page.read_text()
        page.write_text(text.replace("Rig3 in demo", "Wrong in demo"))
        self.assert_code("invalid_evidence", self.build)
        page.write_text(text.replace('id="method.update"', 'id="method.fake"'))
        self.assert_code("invalid_evidence", self.build)
        page.write_text(text)
        for path in (self.root / "target/doc/demo").glob("*.html"):
            path.write_text("<html>No declarations</html>")
        self.assert_code("missing_index", self.build)

    def test_symbol_file_total_bytes_and_html_depth_capacity(self):
        with patch.object(api, "MAX_SYMBOLS", 2):
            self.assert_code("capacity", self.build)
        with patch.object(api, "MAX_FILES", 2):
            self.assert_code("capacity", api.prepare, self.root)
        with patch.object(api, "MAX_TOTAL_BYTES", 20):
            self.assert_code("capacity", api.prepare, self.root)
        with patch.object(api, "MAX_JSON_BYTES", 20):
            self.assert_code("capacity", api._json_bytes, {"value": "x" * 20})
        oversized = self.write("target/large", "x" * (api.MAX_FILE_BYTES + 1))
        self.assert_code("capacity", api._read, oversized)
        self.assert_code("capacity", api._parse, ("<div>" * (api.MAX_DEPTH + 1)).encode())
        self.assertTrue(self.build()["symbols"], "Capacity failure must not retain partial state")

    def test_queries_are_bounded_and_invalid_limits_never_clamp(self):
        self.save_index()
        for query in ("", " ", "a" * (api.MAX_QUERY + 1), "pub fn Rig3", "../Rig3", 5, "\n"):
            self.assert_code("invalid_arguments", self.search, query)
        for limit in (0, -1, 101, 1.5, True):
            self.assert_code("invalid_arguments", self.search, "Rig3", limit=limit)
        self.assert_code("invalid_arguments", self.search, "Rig3", mode="other")
        self.assertEqual(self.search("Rig3", limit=1)["total_matches"], 1)

    def test_local_paths_and_symlinks_cannot_escape(self):
        for path in ("../outside", "/tmp/outside", "bad\\path", "x" * (api.MAX_PATH + 1)):
            self.assert_code("invalid_path", api._local, self.root, path)
        (self.root / "target/link").symlink_to(self.root / "crates/demo/src/lib.rs")
        self.assert_code("invalid_path", api._local, self.root, "target/link")
        self.page("fn.escape.html", document("escape in demo", "Function escape", "pub fn escape()", source="../../../../etc/passwd"))
        self.assert_code("invalid_evidence", self.build)

    def test_all_cli_errors_are_single_json_and_outputs_never_overwrite(self):
        for args, code in (([], "invalid_arguments"), (["search", "Rig3"], "missing_index"),
                           (["search", "Rig3", "--limit", "bad"], "invalid_arguments"),
                           (["unknown"], "invalid_arguments"),
                           (["prepare", "-p", "missing", "--output", "target/missing.json"], "unknown_package")):
            status, report = self.invoke(*args)
            self.assertEqual(status, 2)
            self.assertEqual(report["error"]["code"], code)
        status, report = self.invoke("prepare", "-p", "demo", "--output", "target/new-snapshot.json")
        self.assertEqual(status, 0)
        original = (self.root / "target/new-snapshot.json").read_bytes()
        status, report = self.invoke("prepare", "--output", "target/new-snapshot.json")
        self.assertEqual(report["error"]["code"], "output_exists")
        self.assertEqual((self.root / "target/new-snapshot.json").read_bytes(), original)
        status, report = self.invoke("prepare", "--output", "docs/inputs.json")
        self.assertEqual(status, 2)

    def test_cli_build_and_search_recipe_including_no_partial_output(self):
        self.write("target/snapshot.json", api._json_bytes(self.snapshot).decode())
        status, report = self.invoke("build", "--snapshot", "target/snapshot.json", "--output", "target/index.json")
        self.assertEqual(status, 0, report)
        self.assertEqual(report["symbols"], 5)
        status, report = self.invoke("search", "Rig3::update", "--index", "target/index.json")
        self.assertEqual(status, 0, report)
        self.assertEqual(report["results"][0]["name"], "update")
        self.write("Cargo.lock", "changed")
        status, report = self.invoke("build", "--snapshot", "target/snapshot.json", "--output", "target/bad.json")
        self.assertEqual(status, 2)
        self.assertEqual(report["error"]["code"], "stale_index")
        self.assertFalse((self.root / "target/bad.json").exists())

    def test_subprocess_machine_output_and_no_stderr_traceback(self):
        self.save_index()
        process = subprocess.run([sys.executable, "-B", str(SCRIPT), "--root", str(self.root), "search", "Rig3",
                                  "--index", "target/index.json"], capture_output=True, text=True, timeout=20)
        self.assertEqual(process.returncode, 0, process.stderr + process.stdout)
        self.assertEqual(json.loads(process.stdout)["results"][0]["import_path"], "demo::Rig3")
        self.assertEqual(process.stderr, "")
        failed = subprocess.run([sys.executable, "-B", str(SCRIPT), "--invalid"], capture_output=True, text=True, timeout=20)
        self.assertEqual(failed.returncode, 2)
        self.assertEqual(json.loads(failed.stdout)["error"]["code"], "invalid_arguments")
        self.assertEqual(failed.stderr, "")

    def test_change_during_build_is_detected(self):
        original = api._parse
        changed = False

        def parse(raw):
            nonlocal changed
            result = original(raw)
            if not changed:
                changed = True
                self.write("docs/concurrent.md", "new input")
            return result

        with patch.object(api, "_parse", parse):
            self.assert_code("stale_index", self.build)

    def test_distance_matches_reference_algorithm_exhaustively(self):
        from itertools import product
        words = ["".join(chars) for size in range(4) for chars in product("ab", repeat=size)]
        for left in words:
            for right in words:
                row = list(range(len(right) + 1))
                for i, char in enumerate(left, 1):
                    new = [i]
                    for j, other in enumerate(right, 1):
                        new.append(min(new[-1] + 1, row[j] + 1, row[j - 1] + (char != other)))
                    row = new
                for bound in (1, 2):
                    self.assertEqual(min(api._distance(left, right, bound), bound + 1), min(row[-1], bound + 1))


class ExistingEvidenceTests(unittest.TestCase):
    def test_read_only_existing_rustdoc_format_not_freshness(self):
        root = SCRIPT.parent.parent
        page = root / "target/doc/ferravex_core/session/struct.GameSession.html"
        if not page.exists():
            self.skipTest("No preexisting local Rustdoc; isolated fixtures still cover the format")
        parser = api._parse(api._read(page))
        self.assertEqual(api._identity(parser), ("ferravex_core::session::GameSession", "struct"))
        self.assertIn("pub struct GameSession", "".join(parser.top["signature"]))
        names = {"".join(member["name"]) for member in parser.members}
        self.assertIn("restart", names)
        self.assertNotIn("borrow_mut", names)
        source = root / "target/doc/src/ferravex_core/session.rs.html"
        parsed = api._parse(api._read(source))
        self.assertEqual(parsed.description, "Source of the Rust file `crates/ferravex-core/src/session.rs`.")
        self.assertIn("pub struct GameSession", "".join(parsed.source_text))
        self.assertNotIn('<a href="#', "".join(parsed.source_text))


if __name__ == "__main__":
    unittest.main()
