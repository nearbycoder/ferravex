#!/usr/bin/env python3
"""Check explicitly scoped local documentation without fetching URLs.

Usage::

    python3 -B scripts/check-docs.py docs/QUICKSTART.md --json
    python3 -B scripts/check-docs.py --root dist/site docs --json
    python3 -B scripts/check-docs.py docs/recipe.md --extract target/doc-examples
    python3 -B scripts/check-docs.py docs/recipe.md --check-rust --json

Only fences marked ``rust,check`` (or ``rust check``) are extracted. Optional
attributes are ``edition=2015|2018|2021|2024`` and ``crate-type=lib|bin``; defaults
are 2024 and lib. Supply a complete Rust crate, including a main function for
bin examples. Rustdoc ``# `` hidden lines are included and ``##`` escapes one
hash. There is no implicit wrapping, dependency inference, or prose execution.
An ordinary rust/no_run/ignore/compile_fail or shell fence is never selected.
Unknown attributes on a check fence are errors, not compiler/shell arguments.

Default operation only reads files. --extract creates a NEW directory containing
deterministically named .rs files and manifest.json (including source hashes and
rustc argv, retaining --extern/--rustc when also checking). --check-rust invokes
rustc sequentially with --emit=metadata in a temporary directory, never runs
the resulting code, and never invokes Cargo or
build scripts. Optional --extern NAME=PATH accepts caller-selected .rlib/.rmeta
artifacts (not procedural macro libraries); build matching first-party artifacts
separately. Only allowlisted fence options affect the command. This is not a Rust sandbox:
explicitly selected Rust can use compiler file-reading facilities such as
include_str!, and caller-supplied artifacts/toolchains must be trusted.

Markdown support: top-level backtick/tilde fences, ATX/setext headings, inline
links/images (balanced destination parentheses), single-line reference
definitions and references, autolinks, and HTML href/src/id/name attributes.
Comments, inline/fenced/indented code, and HTML script/style/pre/code contents
are ignored as links. This is a documented subset, not a CommonMark renderer;
nested list/blockquote fences, MDX, templating, and Rustdoc intra-doc links are
not interpreted. --anchor-style site (default) matches build-site.py's ASCII
slug/counter convention; github uses Unicode visible-text heading slugs.
HTML anchors are exact. Other text files support #L12 and #L12-L20 only.

Paths are relative to --root (the checkout by default); at least one is required.
--extract and --extern paths are relative to the invocation working directory.
Root-relative URLs resolve under that root. Outside-root paths/symlinks are
errors; recursive discovery does not follow directory symlinks and skips hidden
directories, target, dist, node_modules and __pycache__ (explicit scopes can
select these). Targets are inspected for anchors but never recursively audited.
Each document is limited to 2 MiB; discovery/target parsing to 4096 documents.
JSON schema_version 1 has files, examples, diagnostics, counts and ok. Diagnostic
locations are one-based; line/column 0 denotes a path/command error. Exit status
is 0 on success, 1 on findings or compiler failures, 2 on invalid CLI options.
"""

import argparse
from bisect import bisect_right
from collections.abc import Sequence
import hashlib
import html
from html.parser import HTMLParser
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parent.parent
MAX_BYTES = 2 * 1024 * 1024
MAX_FILES = 4096
DOC_SUFFIXES = {".md", ".markdown", ".html", ".htm"}
SKIP_DIRS = {"target", "dist", "node_modules", "__pycache__"}
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
ESCAPE = re.compile(r"\\([!\"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])")
LITERAL_HTML = re.compile(r"<(script|style|pre|code)\b[^>]*>", re.I)


def _diagnostic(path, line, code, message, *, column=1, target=None):
    result = dict(severity="error", code=code, path=path, line=line,
                  column=column if line else 0, message=message)
    if target is not None:
        result["target"] = target
    return result


def _blank(text):
    return "".join("\n" if char == "\n" else " " for char in text)


def _mask_code(text):
    # Index equal-length delimiter runs to avoid quadratic regex backtracking
    # on long/unclosed runs. Backticks inside a matched span are literal.
    runs = list(re.finditer(r"`+", text))
    following, by_width = {}, {}
    for index in range(len(runs) - 1, -1, -1):
        width = runs[index].end() - runs[index].start()
        if width in by_width:
            following[index] = by_width[width]
        by_width[width] = index
    pieces, offset, index = [], 0, 0
    while index < len(runs):
        if index not in following:
            index += 1
            continue
        end = following[index]
        pieces.extend((text[offset:runs[index].start()],
                       _blank(text[runs[index].start():runs[end].end()])))
        offset, index = runs[end].end(), end + 1
    pieces.append(text[offset:])
    return "".join(pieces)


def _normalize_reference(label):
    return " ".join(ESCAPE.sub(r"\1", label).split()).casefold()


def _destination(text, start, *, inline):
    """Return (unescaped destination, end) or None; never interpret a command."""
    index = start
    while index < len(text) and text[index].isspace():
        index += 1
    begin = index
    if index < len(text) and text[index] == "<":
        index += 1
        begin = index
        while index < len(text) and text[index] not in ">\n":
            index += 2 if text[index] == "\\" else 1
        if index >= len(text) or text[index] != ">":
            return None
        value = text[begin:index]
        index += 1
    else:
        depth = 0
        while index < len(text):
            char = text[index]
            if char == "\\" and index + 1 < len(text):
                index += 2
                continue
            if char.isspace() or (char == ")" and depth == 0):
                break
            depth += (char == "(") - (char == ")")
            index += 1
        if depth:
            return None
        value = text[begin:index]
    if not inline:
        return html.unescape(ESCAPE.sub(r"\1", value)), index
    while index < len(text) and text[index].isspace():
        index += 1
    if index < len(text) and text[index] in "\"'(" and index > begin:
        closing = ")" if text[index] == "(" else text[index]
        index += 1
        while index < len(text) and text[index] != closing:
            index += 2 if text[index] == "\\" else 1
        index += 1
        while index < len(text) and text[index].isspace():
            index += 1
    if index >= len(text) or text[index] != ")":
        return None
    return html.unescape(ESCAPE.sub(r"\1", value)), index + 1


def _inline_links(text, path, diagnostics):
    offsets = [index for index, char in enumerate(text) if char == "\n"]

    def location(index):
        row = bisect_right(offsets, index)
        return row + 1, index - (offsets[row - 1] if row else -1)

    links, definitions = [], {}
    pattern = re.compile(r"^ {0,3}\[([^]\n]+)\]:[ \t]*(.*)$", re.M)
    parts, end = [], 0
    for match in pattern.finditer(text):
        destination = _destination(match[2], 0, inline=False)
        if destination and destination[0]:
            label = _normalize_reference(match[1])
            if label not in definitions:
                definitions[label] = destination[0]
                line, column = location(match.start())
                links.append((destination[0], line, column))
            parts.extend((text[end:match.start()], _blank(match[0])))
            end = match.end()
    parts.append(text[end:])
    text = "".join(parts)
    stack, index = [], 0
    while index < len(text):
        char = text[index]
        if char == "\\":
            index += 2
            continue
        if char == "[":
            stack.append(index)
        elif char == "]" and stack:
            start = stack.pop()
            label = text[start + 1:index]
            after = index + 1
            if after < len(text) and text[after] == "(":
                destination = _destination(text, after + 1, inline=True)
                if destination:
                    line, column = location(start)
                    links.append((destination[0], line, column))
                    index = destination[1]
                    continue
            elif after < len(text) and text[after] == "[":
                close = text.find("]", after + 1)
                if close >= 0:
                    reference = text[after + 1:close] or label
                    if _normalize_reference(reference) not in definitions:
                        line, column = location(start)
                        diagnostics.append(_diagnostic(
                            path, line, "missing-reference",
                            f"Undefined Markdown reference: {reference}", column=column))
                    index = close + 1
                    continue
        index += 1
    for match in re.finditer(r"<([A-Za-z][\w+.-]*:[^<>\s]*)>", text):
        line, column = location(match.start())
        links.append((html.unescape(match[1]), line, column))
    return links


class _HTML(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.anchors = set()
        self.links = []
        self.literal = None

    def handle_starttag(self, tag, attrs):
        if self.literal:
            return
        attrs = dict(attrs)
        for key in ("id", "name" if tag == "a" else "id"):
            if attrs.get(key):
                self.anchors.add(attrs[key])
        line, column = self.getpos()
        for key in ("href", "src"):
            if attrs.get(key) is not None:
                self.links.append((attrs[key], line, column + 1))
        if tag in {"script", "style", "pre", "code"}:
            self.literal = tag

    def handle_endtag(self, tag):
        if tag == self.literal:
            self.literal = None

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        self.handle_endtag(tag)


def _heading_slug(title, style):
    if style == "site":
        title = title.replace("—", " - ").replace("–", "-")
        return re.sub(r"[^a-z0-9 _-]", "", title.lower()).replace(" ", "-")
    title = re.sub(r"!?\[([^]]*)\]\([^)]*\)", r"\1", title)
    title = html.unescape(re.sub(r"<[^>]*>", "", title))
    title = ESCAPE.sub(r"\1", title).replace("`", "").replace("*", "")
    return "".join(char for char in title.lower()
                   if char.isalnum() or char in " _-").replace(" ", "-")


def _rust_example(path, line, info, body, diagnostics, fence_indent=0):
    tokens = [token for token in re.split(r"[,\s]+", info.strip()) if token]
    if "check" not in tokens:
        return None
    options = {"edition": "2024", "crate-type": "lib"}
    seen = set()
    valid = tokens[:1] == ["rust"] and tokens.count("check") == 1
    for token in tokens[1:]:
        if token == "check":
            continue
        key, separator, value = token.partition("=")
        if (not separator or key in seen or key not in options
                or value not in ({"2015", "2018", "2021", "2024"} if key == "edition"
                                 else {"lib", "bin"})):
            valid = False
        else:
            options[key] = value
            seen.add(key)
    if not valid:
        diagnostics.append(_diagnostic(
            path, line, "invalid-example-marker",
            "Use rust,check with optional edition=2015|2018|2021|2024 and crate-type=lib|bin"))
        return None
    lines, column_offsets = [], []
    for raw in body:
        original_length = len(raw)
        raw = raw[min(fence_indent, len(raw) - len(raw.lstrip(" "))):]
        stripped = raw.lstrip(" ")
        indent = raw[:len(raw) - len(stripped)]
        if stripped.startswith("##"):
            raw = indent + stripped[1:]
        elif stripped.startswith("# ") or stripped == "#":
            raw = indent + stripped[2:]
        lines.append(raw)
        column_offsets.append(original_length - len(raw))
    source = "\n".join(lines) + "\n"
    if not source.strip():
        diagnostics.append(_diagnostic(path, line, "empty-example", "Marked Rust example is empty"))
        return None
    identity = hashlib.sha256(f"{path}:{line}".encode()).hexdigest()[:20]
    return dict(id="doc_" + identity, path=path, line=line, code_line=line + 1,
                edition=options["edition"], crate_type=options["crate-type"],
                source=source, sha256=hashlib.sha256(source.encode()).hexdigest(),
                column_offsets=column_offsets, status="not_checked")


def _markdown(text, path, style):
    lines = text.splitlines()
    prose, examples, diagnostics = [], [], []
    fence = None
    comment = False
    literal = None
    for index, raw in enumerate(lines):
        if fence:
            marker, info, start, body, indent = fence
            if re.fullmatch(r" {0,3}" + re.escape(marker[0]) + "{" + str(len(marker)) + r",}\s*", raw):
                example = _rust_example(path, start, info, body, diagnostics, indent)
                if example:
                    examples.append(example)
                fence = None
            else:
                body.append(raw)
            prose.append("")
            continue
        # Mask HTML comments before recognizing fences, preserving columns.
        visible, offset = [], 0
        code_safe = _mask_code(raw)
        while offset < len(raw):
            if comment:
                end = raw.find("-->", offset)
                end = len(raw) if end < 0 else end + 3
                visible.append(" " * (end - offset))
                comment = not raw[offset:end].endswith("-->")
            else:
                end = code_safe.find("<!--", offset)
                end = len(raw) if end < 0 else end
                visible.append(raw[offset:end])
                comment = end < len(raw)
            offset = end
        line = "".join(visible)
        # Leave raw HTML for HTMLParser, but do not discover fences in it.
        if literal:
            if re.search(r"</" + literal + r"\s*>", line, re.I):
                literal = None
            prose.append(line)
            continue
        if opening := LITERAL_HTML.search(_mask_code(line)):
            if not re.search(r"</" + opening[1] + r"\s*>", line[opening.end():], re.I):
                literal = opening[1].lower()
            prose.append(line)
            continue
        match = FENCE.match(line)
        if match and not (match[1].startswith("`") and "`" in match[2]):
            fence = (match[1], match[2], index + 1, [], len(line) - len(line.lstrip(" ")))
            prose.append("")
        elif line.startswith(("    ", "\t")):
            prose.append("")
        else:
            prose.append(line)
    if fence and "check" in re.split(r"[,\s]+", fence[1].strip()):
        diagnostics.append(_diagnostic(path, fence[2], "unclosed-example", "Marked Rust fence is not closed"))
    visible = "\n".join(prose)
    pieces, offset = [], 0
    for match in re.finditer(r"(<(script|style|pre|code)\b[^>]*>)([\s\S]*?)(</\2\s*>|\Z)",
                             _mask_code(visible), flags=re.I):
        pieces.extend((visible[offset:match.start(3)],
                       _blank(visible[match.start(3):match.end(3)])))
        offset = match.end(3)
    pieces.append(visible[offset:])
    visible = "".join(pieces)
    anchors, counts = set(), {}
    heading_lines = visible.splitlines()
    for index, line in enumerate(heading_lines):
        match = re.match(r"^ {0,3}#{1,6}[ \t]+(.+?)\s*$", line)
        title = match[1] if match else None
        if title is not None and style == "github":
            title = re.sub(r"\s+#+\s*$", "", title)
        elif (title is None and style == "github" and line.strip()
              and index + 1 < len(heading_lines)
              and re.fullmatch(r" {0,3}(?:=+|-+)[ \t]*", heading_lines[index + 1])):
            title = line.strip()
        if title is not None:
            slug = _heading_slug(title, style)
            count = counts.get(slug, 0)
            key = slug + (f"-{count}" if count else "")
            if style == "github":
                while key in anchors:
                    count += 1
                    key = f"{slug}-{count}"
            counts[slug] = count + 1
            anchors.add(key)
    # Backtick spans can cross lines. Unequal delimiter lengths do not close.
    visible = _mask_code(visible)
    parser = _HTML()
    parser.feed(visible)
    parser.close()
    anchors.update(parser.anchors)
    # Attributes are handled by HTMLParser, not the Markdown link parser.
    def mask_tag(match):
        index = match.start() - 1
        while index >= 0 and visible[index].isspace():
            index -= 1
        # <path> is also a Markdown destination, including extensionless files.
        destination = index >= 0 and (visible[index] == "(" or (
            visible[index] == ":" and index > 0 and visible[index - 1] == "]"))
        if destination or re.fullmatch(r"<[A-Za-z][\w+.-]*:[^<>\s]*>", match[0]):
            return match[0]
        return _blank(match[0])

    link_text = re.sub(r"</?[A-Za-z][A-Za-z0-9:-]*(?:\s[^<>\n]*)?/?>", mask_tag, visible)
    links = parser.links + _inline_links(link_text, path, diagnostics)
    return dict(anchors=anchors, links=links, examples=examples, diagnostics=diagnostics)


def _read(path):
    if not path.is_file():
        raise ValueError("Target is not a regular file")
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError(f"File exceeds {MAX_BYTES} bytes")
    return data.decode("utf-8-sig")


def _inside(root, path):
    resolved = path.resolve()
    if not resolved.is_relative_to(root):
        raise ValueError("Path escapes the documentation root")
    return resolved


def _finish(report):
    report["diagnostics"].sort(key=lambda row: (
        row["path"], row["line"], row["column"], row["code"], row["message"]))
    report["counts"]["errors"] = len(report["diagnostics"])
    report["counts"]["checked_examples"] = sum(
        row["status"] != "not_checked" for row in report["examples"])
    report["ok"] = not report["diagnostics"]
    return report


def check_docs(root: Path, paths: Sequence[str | Path], *, anchor_style: str = "site") -> dict:
    """Read only the given files/directories and linked anchor targets.

    Return the JSON schema_version 1 report. No compiler, network access or
    writes occur. A missing/empty scope is a diagnostic, never a full-tree scan.
    """
    report = dict(schema_version=1, ok=True, files=[], examples=[], diagnostics=[],
                  counts=dict(files=0, local_links=0, external_links=0, examples=0,
                              checked_examples=0, errors=0))
    diagnostics = report["diagnostics"]
    try:
        root = Path(root).resolve()
    except (OSError, ValueError, RuntimeError) as error:
        diagnostics.append(_diagnostic(str(root), 0, "invalid-scope", str(error)))
        return _finish(report)
    if anchor_style not in {"site", "github"} or not root.is_dir() or not paths:
        diagnostics.append(_diagnostic("", 0, "invalid-scope", "Supply an existing root, paths and site/github anchor style"))
        return _finish(report)
    selected = set()
    try:
        for scope in paths:
            candidate = Path(scope)
            path = _inside(root, candidate if candidate.is_absolute() else root / candidate)
            if not path.exists():
                diagnostics.append(_diagnostic(str(scope), 0, "missing-scope", "Scope does not exist"))
                continue
            if path.is_file():
                if path.suffix.lower() not in DOC_SUFFIXES:
                    diagnostics.append(_diagnostic(str(scope), 0, "unsupported-scope", "Select Markdown or HTML files"))
                else:
                    selected.add(path)
            elif path.is_dir():
                def walk_error(error):
                    raise error

                for directory, dirs, files in os.walk(path, onerror=walk_error, followlinks=False):
                    dirs[:] = sorted(name for name in dirs if not name.startswith(".")
                                     and name not in SKIP_DIRS
                                     and not (Path(directory) / name).is_symlink())
                    for name in sorted(files):
                        if Path(name).suffix.lower() in DOC_SUFFIXES:
                            selected.add(_inside(root, Path(directory) / name))
                            if len(selected) > MAX_FILES:
                                raise ValueError(f"Scope exceeds {MAX_FILES} documents")
            if len(selected) > MAX_FILES:
                raise ValueError(f"Scope exceeds {MAX_FILES} documents")
    except (OSError, ValueError, RuntimeError) as error:
        diagnostics.append(_diagnostic(str(scope), 0, "invalid-scope", str(error)))
        return _finish(report)
    if not selected:
        diagnostics.append(_diagnostic("", 0, "empty-scope", "No Markdown or HTML documents selected"))
    cache = {}

    def document(path):
        if path not in cache:
            if len(cache) >= MAX_FILES:
                raise ValueError(f"Anchor inspection exceeds {MAX_FILES} documents")
            text = _read(path)
            if path.suffix.lower() in {".md", ".markdown"}:
                result = _markdown(text, path.relative_to(root).as_posix(), anchor_style)
            else:
                parser = _HTML()
                parser.feed(text)
                parser.close()
                result = dict(anchors=parser.anchors, links=parser.links,
                              examples=[], diagnostics=[])
            cache[path] = result
        return cache[path]

    for path in sorted(selected):
        label = path.relative_to(root).as_posix()
        report["files"].append(label)
        try:
            parsed = document(path)
        except (OSError, ValueError, RuntimeError) as error:
            diagnostics.append(_diagnostic(label, 0, "unreadable-document", str(error)))
            continue
        report["examples"].extend(parsed["examples"])
        diagnostics.extend(parsed["diagnostics"])
        for value, line, column in parsed["links"]:
            def fail(code, message):
                diagnostics.append(_diagnostic(label, line, code, message, column=column, target=value))

            try:
                url = urlsplit(value)
                if url.scheme or url.netloc:
                    report["counts"]["external_links"] += 1
                    continue
                report["counts"]["local_links"] += 1
                target_text = unquote(url.path, errors="strict")
                fragment = unquote(url.fragment, errors="strict")
                if "\0" in target_text or "\\" in target_text:
                    raise ValueError("Local URL paths cannot contain NUL or backslashes")
                target = (_inside(root, root / target_text.lstrip("/")) if target_text.startswith("/")
                          else _inside(root, path.parent / target_text) if target_text else path)
                if not target.exists():
                    fail("missing-target", "Local target does not exist")
                    continue
                if not fragment:
                    continue
                if target.is_dir():
                    target = next((target / name for name in ("index.html", "README.md", "index.md")
                                   if (target / name).is_file()), target)
                    target = _inside(root, target)
                if target.suffix.lower() in DOC_SUFFIXES and target.is_file():
                    if fragment not in document(target)["anchors"]:
                        fail("missing-anchor", f"Anchor #{fragment} does not exist")
                elif match := re.fullmatch(r"L([1-9][0-9]*)(?:-L([1-9][0-9]*))?", fragment):
                    first, last = int(match[1]), int(match[2] or match[1])
                    if not (first <= last <= len(_read(target).splitlines())):
                        fail("missing-anchor", "Source line range is outside the target file")
                else:
                    fail("unsupported-anchor", "Anchors require Markdown/HTML or a text #L12[-L20] location")
            except (OSError, ValueError, RuntimeError) as error:
                fail("invalid-target", str(error))
    report["counts"]["files"] = len(report["files"])
    report["counts"]["examples"] = len(report["examples"])
    return _finish(report)


def _rustc_argv(example, externs=()):
    command = ["rustc", example["id"] + ".rs", "--crate-name", example["id"],
               "--edition=" + example["edition"], "--crate-type=" + example["crate_type"],
               "--emit=metadata", "--error-format=json", "-o", example["id"] + ".rmeta"]
    for name, path in externs:
        command.extend(("--extern", f"{name}={path}"))
    for directory in sorted({str(path.parent) for _, path in externs}):
        command.extend(("-L", "dependency=" + directory))
    return command


def extract_examples(report: dict, destination: Path) -> None:
    """Write sources and a deterministic manifest to a new directory; never overwrite.

    Raises OSError/ValueError on output failure. Does not compile or update the
    report; manifest argv are compile-only commands relative to the directory.
    """
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=False)
    entries = []
    for example in report["examples"]:
        filename = example["id"] + ".rs"
        (destination / filename).write_text(example["source"], encoding="utf-8", newline="\n")
        entries.append({key: value for key, value in example.items()
                        if key not in {"source", "status", "check_argv"}}
                       | dict(file=filename, argv=example.get("check_argv", _rustc_argv(example))))
    manifest = dict(schema_version=1, examples=entries)
    (destination / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")


def check_examples(report: dict, *, rustc: str = "rustc",
                   externs: Sequence[str] = (), timeout: float = 30.0) -> None:
    """Type-check marked Rust; append mapped diagnostics and update status in-place.

    externs is a sequence of NAME=PATH strings pointing to prebuilt rlib/rmeta
    files. Only rustc is launched; no examples, shell, Cargo or build scripts run.
    A timeout bounds each compiler call. Compiler logs are read up to MAX_BYTES.
    """
    artifacts = []
    names = set()
    try:
        if not math.isfinite(timeout) or not 0 < timeout <= 300:
            raise ValueError("Compiler timeout must be finite and in (0, 300] seconds")
        if os.sep in rustc or (os.altsep and os.altsep in rustc):
            rustc = str(Path(rustc).resolve())
        for value in externs:
            name, separator, filename = value.partition("=")
            artifact = Path(filename).resolve()
            if (not separator or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", name)
                    or name in names or artifact.suffix not in {".rlib", ".rmeta"}
                    or not artifact.is_file()):
                raise ValueError("--extern needs unique NAME=PATH entries to existing .rlib/.rmeta files")
            artifacts.append((name, artifact))
            names.add(name)
    except (OSError, ValueError, RuntimeError) as error:
        report["diagnostics"].append(_diagnostic("", 0, "invalid-compiler-options", str(error)))
        _finish(report)
        return
    artifacts.sort()
    for example in report["examples"]:
        try:
            with tempfile.TemporaryDirectory(prefix="ferravex-doc-check-") as temporary:
                directory = Path(temporary)
                source = directory / (example["id"] + ".rs")
                source.write_text(example["source"], encoding="utf-8", newline="\n")
                command = _rustc_argv(example, artifacts)
                command[0] = rustc
                example["check_argv"] = command
                with tempfile.TemporaryFile() as errors:
                    result = subprocess.run(command, cwd=directory, stdin=subprocess.DEVNULL,
                                            stdout=subprocess.DEVNULL, stderr=errors,
                                            timeout=timeout, check=False, shell=False)
                    errors.seek(0)
                    output = errors.read(MAX_BYTES).decode("utf-8", errors="replace")
                example["status"] = "passed" if result.returncode == 0 else "failed"
                if result.returncode:
                    mapped = False
                    for raw in output.splitlines():
                        try:
                            error = json.loads(raw)
                        except ValueError:
                            continue
                        if not isinstance(error, dict) or error.get("level") != "error":
                            continue
                        spans = [span for span in error.get("spans", [])
                                 if isinstance(span, dict) and span.get("is_primary")
                                 and span.get("file_name") == source.name
                                 and isinstance(span.get("line_start"), int)
                                 and 1 <= span["line_start"] <= len(example["column_offsets"])
                                 and isinstance(span.get("column_start"), int)]
                        if not spans:
                            continue
                        span = spans[0]
                        report["diagnostics"].append(_diagnostic(
                            example["path"], example["code_line"] + span["line_start"] - 1,
                            "rust-compile", error.get("message", "Rust compilation failed"),
                            column=span["column_start"]
                            + example["column_offsets"][span["line_start"] - 1]))
                        mapped = True
                    if not mapped:
                        report["diagnostics"].append(_diagnostic(
                            example["path"], example["line"], "rust-compile",
                            f"rustc exited {result.returncode}: {output[:4000].strip()}"))
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            example["status"] = "failed"
            report["diagnostics"].append(_diagnostic(
                example["path"], example["line"], "rust-check-failed", str(error)))
            # A missing compiler/invalid invocation affects all remaining blocks.
            if isinstance(error, OSError):
                break
    _finish(report)


class _Parser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def main(argv: Sequence[str] | None = None) -> int:
    """CLI entry point; explicit file/directory scopes are required."""
    argv = list(sys.argv[1:] if argv is None else argv)
    parser = _Parser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter,
                     allow_abbrev=False)
    parser.add_argument("paths", nargs="+", help="Markdown/HTML files or directories, relative to root")
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--json", action="store_true", help="emit only a schema_version 1 JSON report")
    parser.add_argument("--anchor-style", choices=("site", "github"), default="site")
    parser.add_argument("--extract", type=Path, help="write sources/manifest to a NEW directory")
    parser.add_argument("--check-rust", action="store_true", help="type-check marked examples; never run them")
    parser.add_argument("--rustc", default="rustc", help="compiler executable (one argv element)")
    parser.add_argument("--extern", action="append", default=[], metavar="NAME=PATH")
    parser.add_argument("--timeout", type=float, default=30.0, help="compiler seconds per example, at most 300")
    exit_code = 0
    json_output = "--json" in argv
    try:
        args = parser.parse_args(argv)
        json_output = args.json
        if (args.extern or args.rustc != "rustc" or args.timeout != 30.0) and not args.check_rust:
            parser.error("--extern, --rustc and --timeout require --check-rust")
        report = check_docs(args.root, args.paths, anchor_style=args.anchor_style)
        if args.check_rust:
            check_examples(report, rustc=args.rustc, externs=args.extern, timeout=args.timeout)
        if args.extract:
            try:
                extract_examples(report, args.extract)
            except (OSError, ValueError) as error:
                report["diagnostics"].append(_diagnostic(str(args.extract), 0, "extract-failed", str(error)))
                _finish(report)
    except ValueError as error:
        report = dict(schema_version=1, ok=False, files=[], examples=[],
                      counts=dict(files=0, local_links=0, external_links=0, examples=0,
                                  checked_examples=0, errors=1),
                      diagnostics=[_diagnostic("", 0, "usage", str(error))])
        exit_code = 2
    if json_output:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        for diagnostic in report["diagnostics"]:
            print(f"{diagnostic['path']}:{diagnostic['line']}:{diagnostic['column']}: "
                  f"{diagnostic['code']}: {diagnostic['message']}")
        counts = report["counts"]
        print(f"Checked {counts['files']} documents, {counts['local_links']} local links; "
              f"{counts['examples']} marked Rust examples ({counts['checked_examples']} checked); "
              f"{counts['errors']} errors")
    return exit_code or (0 if report["ok"] else 1)


if __name__ == "__main__":
    raise SystemExit(main())
