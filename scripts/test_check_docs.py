#!/usr/bin/env python3
"""Source-only documentation checker regressions; stdlib and isolated fixtures.

The real compiler tests use rustc if installed, never Cargo or repository
build output. Every extracted file and compiler artifact lives in a temporary
directory. All other compiler interactions are mocked.
"""

from contextlib import redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("check-docs.py")
SPEC = importlib.util.spec_from_file_location("check_docs", SCRIPT)
docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(docs)


class DocsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ferravex docs tests ")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / "root"
        self.root.mkdir()

    def write(self, path, text):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")
        return destination

    def check(self, *paths, **kwargs):
        return docs.check_docs(self.root, paths or ["guide.md"], **kwargs)

    def codes(self, report):
        return [row["code"] for row in report["diagnostics"]]

    def invoke(self, *args):
        output = io.StringIO()
        with redirect_stdout(output):
            status = docs.main(["--root", str(self.root), "--json", *args])
        return status, json.loads(output.getvalue())

    def example(self, body="pub fn answer() -> u8 { 42 }", marker="rust,check"):
        return f"```{marker}\n{body}\n```\n"

    def test_local_links_fragments_queries_root_paths_and_images(self):
        self.write("guide.md", """# Start
[same](#start) [root](/other.md#hello) [query](other.md?q=x#hello)
![image](images/a%20b.png "Caption") [directory](folder/)
[empty]() [top](#) [angle](<images/a b.png>)
""")
        self.write("other.md", "## Hello\n")
        self.write("images/a b.png", "opaque")
        (self.root / "folder").mkdir()
        report = self.check()
        self.assertTrue(report["ok"], report["diagnostics"])
        self.assertEqual(report["counts"]["local_links"], 8)
        self.assertEqual(report["files"], ["guide.md"])

    def test_broken_targets_anchors_and_locations(self):
        self.write("guide.md", "# Start\n[missing](lost.md)\n  [anchor](#lost)\n")
        report = self.check()
        self.assertFalse(report["ok"])
        self.assertEqual(self.codes(report), ["missing-target", "missing-anchor"])
        self.assertEqual([(row["path"], row["line"], row["column"], row["target"])
                          for row in report["diagnostics"]],
                         [("guide.md", 2, 1, "lost.md"), ("guide.md", 3, 3, "#lost")])

    def test_site_slugs_match_existing_renderer_convention(self):
        # Copied contract only: importing build-site.py would load site catalogs.
        self.write("guide.md", """# `FrameStep` & **ticks**
## Café — timing
## Repeat
## Repeat
[one](#framestep--ticks) [two](#caf-----timing) [three](#repeat-1)
""")
        self.assertTrue(self.check()["ok"])

    def test_github_heading_style_unicode_setext_and_collision(self):
        self.write("guide.md", """# Café & `Rust` ###
Title
-----
## Same
## Same-1
## Same
## [Hello](guide.md) *world*
[a](#caf%C3%A9--rust) [b](#title) [c](#same-2) [d](#hello-world)
""")
        report = self.check(anchor_style="github")
        self.assertTrue(report["ok"], report["diagnostics"])
        self.assertIn("missing-anchor", self.codes(self.check()))

    def test_html_ids_named_anchors_entities_and_ignored_literals(self):
        self.write("guide.html", """<!doctype html><h1 id="Hello &amp; world">Hi</h1>
<a name="legacy"></a><a href="#Hello%20%26%20world">hello</a><a href="#legacy">old</a>
<!-- <img src="missing.png"><h2 id="comment"> -->
<script>const fake = '<a href="missing">';</script>
<pre><a href="missing">not a link</a></pre>
<a href="other.html#target">actual</a>
""")
        self.write("other.html", '<h2 id="target">Other</h2>')
        report = self.check("guide.html")
        self.assertTrue(report["ok"], report["diagnostics"])
        self.assertEqual(report["counts"]["local_links"], 3)

    def test_markdown_html_anchors_and_links_after_code(self):
        self.write("guide.md", """<a id="custom"></a>
<pre>
[fake](missing)
```rust,check
invalid rust
```
</pre>
<code>[another](missing)</code>
[real](#custom)
<a href="other.html#target">HTML link</a>
""")
        self.write("other.html", '<h2 id="target">Other</h2>')
        report = self.check()
        self.assertTrue(report["ok"], report["diagnostics"])
        self.assertEqual(report["counts"]["local_links"], 2)
        self.assertFalse(report["examples"])

    def test_fenced_indented_inline_code_and_comments_are_not_links(self):
        self.write("guide.md", """# Real
`[inline](missing)` and `` [double](missing) ` more ``
~~~text
[fence](missing)
# Fake
~~~
    [indented](missing)
<!-- [comment](missing)
```rust,check
invalid rust
```
-->
[real](#real)
""")
        report = self.check()
        self.assertTrue(report["ok"], report["diagnostics"])
        self.assertEqual(report["counts"]["local_links"], 1)
        self.assertFalse(report["examples"])

    def test_inline_html_examples_do_not_hide_following_links_or_rust_fences(self):
        self.write("guide.md", "# Using `<pre>`\nLiteral `<!--` and `<code>` markers.\n"
                   "[actual](#using-pre) [broken](missing)\n" + self.example())
        report = self.check()
        self.assertEqual(self.codes(report), ["missing-target"])
        self.assertEqual(report["counts"]["local_links"], 2)
        self.assertEqual(len(report["examples"]), 1)

    def test_inline_code_matches_exact_delimiter_runs_and_handles_long_unclosed_runs(self):
        self.assertEqual(docs._mask_code("before ``a ` b`` after `c`"),
                         "before " + " " * 9 + " after " + " " * 3)
        # This unclosed run previously risked quadratic regex backtracking.
        unclosed = "`" * 20000
        self.assertEqual(docs._mask_code(unclosed), unclosed)

    def test_references_collapsed_shortcut_and_missing_explicit_reference(self):
        self.write("guide.md", """[full][My Ref] [collapsed][] [My Ref] [ordinary prose]
[MY REF]: other.md#ok "title"
[collapsed]: <other.md#ok>
[bad][unknown]
""")
        self.write("other.md", "# Ok\n")
        report = self.check()
        self.assertEqual(self.codes(report), ["missing-reference"])
        self.assertEqual(report["diagnostics"][0]["line"], 4)
        self.assertEqual(report["counts"]["local_links"], 2)

    def test_reference_definition_uses_first_destination(self):
        self.write("guide.md", "[link][x]\n[x]: other.md\n[X]: missing\n")
        self.write("other.md", "")
        self.assertTrue(self.check()["ok"])

    def test_angle_destinations_with_extensionless_paths_are_not_html(self):
        self.write("guide.md", "[a](<present>) [b](<missing>)\n[ref]: <present>\n")
        self.write("present", "text")
        report = self.check()
        self.assertEqual(self.codes(report), ["missing-target"])
        self.assertEqual(report["diagnostics"][0]["target"], "missing")
        self.assertEqual(report["counts"]["local_links"], 3)

    def test_balanced_and_escaped_destinations_nested_labels_titles(self):
        self.write("guide.md", r'''[one](a(b).md "caption")
[nested [label]](a\(b\).md 'title')
[line](
a(b).md
)
\[not a link](missing)
''')
        self.write("a(b).md", "# Present")
        report = self.check()
        self.assertTrue(report["ok"], report["diagnostics"])
        self.assertEqual(report["counts"]["local_links"], 3)

    def test_external_schemes_and_protocol_relative_urls_never_fetch(self):
        self.write("guide.md", """[web](https://invalid.example/404#no)
[mail](mailto:nobody@example.com) ![data](data:image/png;base64,abc)
[relative](//invalid.example/path) <https://invalid.example/>
""")
        with patch.object(docs.subprocess, "run") as run:
            report = self.check()
        self.assertTrue(report["ok"])
        self.assertEqual(report["counts"]["external_links"], 5)
        run.assert_not_called()

    def test_directory_index_anchor_and_source_line_ranges(self):
        self.write("guide.md", "[index](folder/#spot) [readme](crate/#readme) "
                   "[source](lib.rs#L1-L2) [bad](lib.rs#L3) [reverse](lib.rs#L2-L1)")
        self.write("folder/index.html", '<main id="spot"></main>')
        self.write("crate/README.md", "# Readme")
        self.write("lib.rs", "// first\n// second\n")
        report = self.check()
        self.assertEqual(self.codes(report), ["missing-anchor", "missing-anchor"])

    def test_unsupported_fragments_are_explicit_errors(self):
        self.write("guide.md", "[source](lib.rs#symbol)")
        self.write("lib.rs", "// source\n")
        self.assertEqual(self.codes(self.check()), ["unsupported-anchor"])

    def test_linked_targets_do_not_expand_audit_or_extract_examples(self):
        self.write("guide.md", "[other](other.md#present)")
        self.write("other.md", "# Present\n[bad](missing)\n" + self.example())
        report = self.check()
        self.assertTrue(report["ok"])
        self.assertEqual(report["files"], ["guide.md"])
        self.assertFalse(report["examples"])

    def test_explicit_scope_dedup_sorted_discovery_and_ignored_directories(self):
        self.write("docs/b.md", "# B")
        self.write("docs/a.MD", "# A")
        self.write("unrelated.md", "[bad](missing)")
        for directory in ("target", "dist", ".git", "node_modules", "__pycache__"):
            self.write(f"docs/{directory}/bad.md", "[bad](missing)")
        report = self.check("docs", "docs/b.md")
        self.assertTrue(report["ok"])
        self.assertEqual(report["files"], ["docs/a.MD", "docs/b.md"])
        self.assertEqual(self.codes(self.check("docs/target")), ["missing-target"])

    def test_missing_empty_unsupported_and_outside_scopes(self):
        self.write("other.txt", "hello")
        (self.root / "empty").mkdir()
        for paths, code in (([], "invalid-scope"), (["missing"], "missing-scope"),
                            (["empty"], "empty-scope"), (["other.txt"], "unsupported-scope"),
                            (["../"], "invalid-scope")):
            with self.subTest(paths=paths):
                self.assertIn(code, self.codes(docs.check_docs(self.root, paths)))

    def test_traversal_encoded_paths_nul_and_symlink_escape(self):
        outside = self.base / "outside.md"
        outside.write_text("# Outside", encoding="utf-8")
        (self.root / "escape.md").symlink_to(outside)
        self.write("guide.md", "[a](../outside.md) [b](%2e%2e/outside.md) "
                   "[c](escape.md) [d](%00) [e](%5cetc%5cpasswd)")
        report = self.check()
        self.assertEqual(self.codes(report), ["invalid-target"] * 5)
        self.assertEqual(self.codes(self.check("escape.md")), ["invalid-scope"])

    def test_directory_symlink_cycles_not_followed(self):
        self.write("docs/a.md", "# A")
        (self.root / "docs/cycle").symlink_to(self.root / "docs", target_is_directory=True)
        self.assertEqual(self.check("docs")["files"], ["docs/a.md"])

    def test_size_utf8_file_and_anchor_document_budgets(self):
        self.write("guide.md", "x" * 65)
        with patch.object(docs, "MAX_BYTES", 64):
            self.assertEqual(self.codes(self.check()), ["unreadable-document"])
        (self.root / "guide.md").write_bytes(b"\xff")
        self.assertEqual(self.codes(self.check()), ["unreadable-document"])
        self.write("guide.md", "[other](other.md#here)")
        self.write("other.md", "# Here")
        with patch.object(docs, "MAX_FILES", 1):
            self.assertEqual(self.codes(self.check(".")), ["invalid-scope"])
            self.assertEqual(self.codes(self.check()), ["invalid-target"])

    @unittest.skipUnless(hasattr(os, "mkfifo"), "named pipes unavailable")
    def test_nonregular_documents_and_line_targets_never_open(self):
        os.mkfifo(self.root / "pipe.md")
        report = self.check(".")
        self.assertEqual(self.codes(report), ["unreadable-document"])
        os.mkfifo(self.root / "pipe.rs")
        self.write("guide.md", "[pipe](pipe.rs#L1)")
        self.assertEqual(self.codes(self.check()), ["invalid-target"])

    def test_only_opted_in_rust_fences_extract_and_no_process_runs(self):
        self.write("guide.md", self.example("unselected", "rust")
                   + self.example("unselected", "rust,no_run")
                   + self.example("touch /should-not-exist", "sh")
                   + self.example("unselected", "rust,compile_fail")
                   + self.example("unselected", "rust,ignore")
                   + self.example())
        with patch.object(docs.subprocess, "run") as run:
            report = self.check()
        self.assertTrue(report["ok"])
        self.assertEqual(len(report["examples"]), 1)
        example = report["examples"][0]
        self.assertEqual((example["edition"], example["crate_type"], example["status"]),
                         ("2024", "lib", "not_checked"))
        self.assertEqual((example["line"], example["code_line"]), (16, 17))
        self.assertEqual(example["sha256"], hashlib.sha256(example["source"].encode()).hexdigest())
        run.assert_not_called()

    def test_tilde_long_fences_indentation_and_rustdoc_hidden_lines(self):
        self.write("guide.md", """  ~~~~rust check edition=2021 crate-type=bin
  # fn main() {
  #     let value = 1;
  ##[allow(unused)]
  let text = "~~~";
  # }
  ~~~~~
""")
        report = self.check()
        self.assertTrue(report["ok"])
        example = report["examples"][0]
        self.assertEqual((example["edition"], example["crate_type"]), ("2021", "bin"))
        self.assertEqual(example["source"], 'fn main() {\n    let value = 1;\n'
                         '#[allow(unused)]\nlet text = "~~~";\n}\n')
        self.assertEqual(example["column_offsets"], [4, 4, 3, 2, 4])

    def test_invalid_markers_never_become_compiler_options(self):
        for marker in ("sh,check", "rust,check,--cfg=evil", "rust,check,ignore",
                       "rust,check,compile_fail", "rust,check,edition=2099",
                       "rust,check,crate-type=proc-macro", "rust,check,check",
                       "rust,check,edition=2021,edition=2024", "rust,check;touch /tmp/no"):
            with self.subTest(marker=marker):
                self.write("guide.md", self.example(marker=marker))
                report = self.check()
                self.assertFalse(report["examples"])
                # 'check;touch' is not the explicit standalone check marker.
                if "check;" not in marker:
                    self.assertEqual(self.codes(report), ["invalid-example-marker"])

    def test_empty_and_unclosed_marked_fences_are_errors(self):
        self.write("guide.md", self.example("# \n ") + "```rust,check\npub fn f() {}\n~~\n")
        report = self.check()
        self.assertEqual(self.codes(report), ["empty-example", "unclosed-example"])
        self.assertFalse(report["examples"])

    def test_manifest_and_source_are_reproducible_without_overwriting(self):
        self.write("guide.md", self.example())
        report = self.check()
        output_a, output_b = self.base / "a", self.base / "b"
        docs.extract_examples(report, output_a)
        docs.extract_examples(self.check(), output_b)
        self.assertEqual({p.name: p.read_bytes() for p in output_a.iterdir()},
                         {p.name: p.read_bytes() for p in output_b.iterdir()})
        manifest = json.loads((output_a / "manifest.json").read_text())
        self.assertEqual(manifest["schema_version"], 1)
        entry = manifest["examples"][0]
        self.assertIn("--emit=metadata", entry["argv"])
        self.assertEqual(entry["file"], entry["id"] + ".rs")
        with self.assertRaises(FileExistsError):
            docs.extract_examples(report, output_a)

    def test_default_and_extract_modes_never_call_compiler(self):
        self.write("guide.md", self.example("invalid rust"))
        with patch.object(docs.subprocess, "run") as run:
            status, report = self.invoke("guide.md", "--extract", str(self.base / "out"))
        self.assertEqual(status, 0)
        self.assertEqual(report["examples"][0]["status"], "not_checked")
        run.assert_not_called()

    def test_compiler_uses_literal_argv_metadata_timeout_and_no_shell(self):
        self.write("guide.md", self.example("fn main() { panic!(\"never run\"); }",
                                             "rust,check,crate-type=bin"))
        artifact = self.write("built artifacts/libferravex_core.rlib", "fixture")
        report = self.check()

        def run(command, **kwargs):
            self.assertEqual(command[0], "rustc; this is one executable name")
            self.assertIn("--emit=metadata", command)
            self.assertIn("--error-format=json", command)
            self.assertIn("--crate-type=bin", command)
            self.assertIn(f"ferravex_core={artifact}", command)
            self.assertIn("dependency=" + str(artifact.parent), command)
            self.assertEqual(kwargs["timeout"], 7)
            self.assertIs(kwargs["shell"], False)
            self.assertIs(kwargs["check"], False)
            self.assertEqual((kwargs["cwd"] / command[1]).read_text(), report["examples"][0]["source"])
            return subprocess.CompletedProcess(command, 0)

        with patch.object(docs.subprocess, "run", side_effect=run) as compiler:
            docs.check_examples(report, rustc="rustc; this is one executable name",
                                externs=[f"ferravex_core={artifact}"], timeout=7)
        self.assertEqual(compiler.call_count, 1)
        self.assertTrue(report["ok"])
        self.assertEqual(report["examples"][0]["status"], "passed")
        extracted = self.base / "with-externs"
        docs.extract_examples(report, extracted)
        entry = json.loads((extracted / "manifest.json").read_text())["examples"][0]
        self.assertEqual(entry["argv"], report["examples"][0]["check_argv"])
        self.assertIn(f"ferravex_core={artifact}", entry["argv"])

    def test_compiler_json_diagnostics_map_to_markdown_lines(self):
        self.write("guide.md", "Intro\n\n" + self.example("pub fn wrong() -> u32 {\n    false\n}"))
        report = self.check()

        def run(command, **kwargs):
            message = dict(level="error", message="expected u32, found bool", spans=[dict(
                file_name=command[1], line_start=2, column_start=5, is_primary=True)])
            kwargs["stderr"].write((json.dumps(message) + "\n").encode())
            return subprocess.CompletedProcess(command, 1)

        with patch.object(docs.subprocess, "run", side_effect=run):
            docs.check_examples(report)
        self.assertEqual(self.codes(report), ["rust-compile"])
        error = report["diagnostics"][0]
        self.assertEqual((error["path"], error["line"], error["column"]), ("guide.md", 5, 5))
        self.assertEqual(report["counts"]["checked_examples"], 1)

    def test_compiler_columns_include_removed_fence_indent_and_hidden_prefix(self):
        self.write("guide.md", "  ```rust,check\n  # pub fn bad() -> u32 { false }\n  ```\n")
        report = self.check()

        def run(command, **kwargs):
            kwargs["stderr"].write(json.dumps(dict(level="error", message="bad", spans=[dict(
                file_name=command[1], line_start=1, column_start=23, is_primary=True)])).encode())
            return subprocess.CompletedProcess(command, 1)

        with patch.object(docs.subprocess, "run", side_effect=run):
            docs.check_examples(report)
        self.assertEqual(report["diagnostics"][0]["column"], 27)

    def test_timeout_continues_to_later_examples_missing_compiler_stops(self):
        self.write("guide.md", self.example() + self.example())
        report = self.check()
        outcomes = [subprocess.TimeoutExpired("rustc", 1), subprocess.CompletedProcess([], 0)]
        with patch.object(docs.subprocess, "run", side_effect=outcomes) as run:
            docs.check_examples(report)
        self.assertEqual(run.call_count, 2)
        self.assertEqual([row["status"] for row in report["examples"]], ["failed", "passed"])
        report = self.check()
        with patch.object(docs.subprocess, "run", side_effect=FileNotFoundError("rustc")) as run:
            docs.check_examples(report)
        self.assertEqual(run.call_count, 1)
        self.assertEqual([row["status"] for row in report["examples"]], ["failed", "not_checked"])

    def test_compiler_timeout_missing_tool_and_non_json_failures(self):
        self.write("guide.md", self.example())
        for error in (subprocess.TimeoutExpired("rustc", 1), FileNotFoundError("rustc missing")):
            report = self.check()
            with patch.object(docs.subprocess, "run", side_effect=error):
                docs.check_examples(report)
            self.assertEqual(self.codes(report), ["rust-check-failed"])
            self.assertEqual(report["examples"][0]["status"], "failed")

        def run(command, **kwargs):
            kwargs["stderr"].write(b"compiler unavailable\n")
            return subprocess.CompletedProcess(command, 127)

        report = self.check()
        with patch.object(docs.subprocess, "run", side_effect=run):
            docs.check_examples(report)
        self.assertEqual(self.codes(report), ["rust-compile"])
        self.assertIn("compiler unavailable", report["diagnostics"][0]["message"])

    def test_invalid_compiler_options_and_proc_macros_rejected(self):
        self.write("guide.md", self.example())
        artifact = self.write("libmacro.so", "not loaded")
        valid = self.write("libcore.rlib", "not loaded")
        for options in ({"timeout": 0}, {"timeout": float("nan")}, {"timeout": 301},
                        {"externs": [f"macro={artifact}"]}, {"externs": ["core=missing.rlib"]},
                        {"externs": [f"a={valid}", f"a={valid}"]},
                        {"externs": [f"--cfg={valid}"]}):
            with self.subTest(options=options):
                report = self.check()
                with patch.object(docs.subprocess, "run") as run:
                    docs.check_examples(report, **options)
                self.assertEqual(self.codes(report), ["invalid-compiler-options"])
                run.assert_not_called()

    def test_cli_schema_exit_status_and_errors_are_json(self):
        self.write("guide.md", "# Guide\n")
        status, report = self.invoke("guide.md")
        self.assertEqual((status, report["schema_version"], report["ok"]), (0, 1, True))
        self.assertEqual(self.invoke("missing.md")[0], 1)
        self.assertEqual(self.invoke()[0], 2)
        self.assertEqual(self.invoke("--unknown")[0], 2)
        self.assertEqual(self.invoke("guide.md", "--extern", "a=x")[0], 2)
        self.assertEqual(self.invoke("guide.md", "--timeout", "invalid")[0], 2)
        self.assertEqual(self.invoke("guide.md", "--extract", str(self.root))[0], 1)

    @unittest.skipUnless(shutil.which("rustc"), "rustc is not installed")
    def test_real_rustc_checks_valid_invalid_and_never_runs_main(self):
        sentinel = self.base / "should not exist"
        body = f"fn main() {{ std::fs::write({json.dumps(str(sentinel))}, b\"bad\").unwrap(); }}"
        self.write("guide.md", self.example(body, "rust,check,crate-type=bin")
                   + self.example("pub fn bad() -> u32 { false }"))
        status, report = self.invoke("guide.md", "--check-rust")
        self.assertEqual(status, 1)
        self.assertEqual([row["status"] for row in report["examples"]], ["passed", "failed"])
        self.assertFalse(sentinel.exists())
        self.assertEqual(self.codes(report), ["rust-compile"])
        self.assertEqual(report["diagnostics"][0]["line"], 5)

    @unittest.skipUnless(shutil.which("rustc"), "rustc is not installed")
    def test_extracted_manifest_replays_with_real_rustc(self):
        self.write("guide.md", self.example("pub fn answer() -> u8 { 42 }"))
        output = self.base / "extracted"
        docs.extract_examples(self.check(), output)
        entry = json.loads((output / "manifest.json").read_text())["examples"][0]
        result = subprocess.run(entry["argv"], cwd=output, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((output / (entry["id"] + ".rmeta")).is_file())

    @unittest.skipUnless(shutil.which("rustc"), "rustc is not installed")
    def test_real_prebuilt_first_party_metadata_can_be_supplied(self):
        source = self.write("fixture.rs", "pub struct Value(pub u32);\n")
        artifact = self.root / "libfixture.rmeta"
        result = subprocess.run(["rustc", str(source), "--crate-name", "fixture",
                                 "--crate-type=lib", "--edition=2024", "--emit=metadata",
                                 "-o", str(artifact)], capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.write("guide.md", self.example("pub fn value() -> fixture::Value { fixture::Value(42) }"))
        status, report = self.invoke("guide.md", "--check-rust", "--extern", f"fixture={artifact}")
        self.assertEqual(status, 0, report["diagnostics"])
        self.assertEqual(report["examples"][0]["status"], "passed")


if __name__ == "__main__":
    unittest.main()
