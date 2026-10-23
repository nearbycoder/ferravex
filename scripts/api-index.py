#!/usr/bin/env python3
"""Bounded, local Rustdoc symbol search (stdlib only, Python 3.11+).

Parent/integration recipe, from the checkout root (use a NEW target directory):

    python3 -B scripts/api-index.py prepare -p ferravex-core --output target/api-inputs.json
    cargo doc -p ferravex-core --lib --no-deps --offline --locked -j 2 --target-dir target/api-docs
    python3 -B scripts/api-index.py build --snapshot target/api-inputs.json --docs target/api-docs/doc --output target/api-index.json
    python3 -B scripts/api-index.py search GameSession --index target/api-index.json
    python3 -B scripts/api-index.py search GameSesion --mode fuzzy --limit 5

All commands emit one schema_version=1 JSON object. Errors exit 2, including
invalid arguments; a valid empty search exits 0. Nothing invokes Cargo, the
network, or JavaScript. Output files must be new; stdout is never mixed with logs.
``prepare`` snapshots first-party build inputs BEFORE the caller generates docs.
Pass the SAME packages/features/target to Cargo as to prepare. Configuration is
caller-declared, not inferred feature requirements or proof Cargo succeeded.
``build`` requires newer HTML, unchanged inputs, and rendered source matching
checkout source. Search checks inputs, artifact inventory/hashes and index
checksum on EVERY call; there is no stale override or persistent in-memory cache.
Rebuild into new paths to reset an index. Existing, unattested docs cannot be
retroactively declared fresh by taking a snapshot after their generation.

The supported evidence format is library Rustdoc HTML (including dist/site/api), with
Rustdoc titles, main headings, declarations, member headings and source links.
Names never come from filenames, source regexes, or prose/search snippets. The
existing site corpus lacks build-input provenance and isolated signatures, so
it is deliberately not accepted as a standalone symbol database. Item imports
are their documented paths; associated items expose their owner as import_path
and their member path as qualified_path. Trait implementation/blanket/deref
members and fields/variants are excluded. Module/crate signatures are null when
Rustdoc renders no declaration. Raw identifiers and reexport mirrors survive.

Freshness covers all files under crates/, examples/, docs/, .cargo/ plus root
Cargo/toolchain/build files and this parser, excluding generated target/dist,
.git and __pycache__ directories. Symlinks are rejected. Members outside crates/
or examples/ are unsupported. Untracked files within that scope count too.
External include files, environment variables, compiler binaries and a dishonest
caller's build/configuration attestation cannot be certified by this tool.
Bounds are fixed constants below, including input bytes, files, HTML nesting,
symbol count, path/query length and result/signature length. Exceeding a bound
fails the whole operation, never silently produces an incomplete index.
"""

import argparse
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import stat
import sys
import time
import tomllib
from urllib.parse import quote, unquote, urlsplit

SCHEMA_VERSION = 1
MAX_FILES = 20000
MAX_FILE_BYTES = 8 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
MAX_JSON_BYTES = 64 * 1024 * 1024
MAX_SYMBOLS = 50000
MAX_DEPTH = 128
MAX_PATH = 512
MAX_QUERY = 160
MAX_RESULTS = 100
MAX_SIGNATURE = 640
MAX_AVAILABILITY = 320
SKIP_DIRS = {"target", "dist", ".git", "__pycache__"}
ROOT_INPUTS = ("Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml", "build.rs")
INPUT_DIRS = ("crates", "examples", "docs", ".cargo")
KINDS = {"Struct": "struct", "Enum": "enum", "Trait": "trait", "Function": "function",
         "Type Alias": "type", "Constant": "constant", "Static": "static",
         "Macro": "macro", "Union": "union", "Module": "module", "Crate": "crate"}
MEMBER_KINDS = {"method": "method", "tymethod": "required_method",
                "associatedconstant": "associated_constant", "associatedtype": "associated_type"}
VOID_TAGS = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link",
             "meta", "param", "source", "track", "wbr"}


class IndexError(ValueError):
    """A machine-readable failure; ``code`` is stable within schema version 1."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def _fail(code, message):
    raise IndexError(code, message)


def _compact(value):
    return " ".join(value.split())


def _json_bytes(value):
    chunks = []
    size = 1
    encoder = json.JSONEncoder(ensure_ascii=True, sort_keys=True, separators=(",", ":"))
    for part in encoder.iterencode(value):
        raw = part.encode()
        size += len(raw)
        if size > MAX_JSON_BYTES:
            _fail("capacity", "JSON byte limit exceeded")
        chunks.append(raw)
    return b"".join(chunks) + b"\n"


def _sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _sealed(value):
    return dict(value, payload_sha256=_sha(_json_bytes(value)))


def _checked_envelope(value, kind):
    if (not isinstance(value, dict) or type(value.get("schema_version")) is not int
            or value["schema_version"] != SCHEMA_VERSION or value.get("kind") != kind):
        _fail("invalid_index", f"Expected {kind} schema_version={SCHEMA_VERSION}")
    fields = ({"inputs", "packages", "configuration", "prepared_ns"} if kind == "ferravex.api-inputs"
              else {"snapshot", "docs_root", "inventory", "artifacts", "symbols", "rustdoc_versions", "skipped_pages"})
    if set(value) != fields | {"schema_version", "kind", "payload_sha256"}:
        _fail("invalid_index", "Unexpected or missing envelope fields")
    body = {key: item for key, item in value.items() if key != "payload_sha256"}
    if value.get("payload_sha256") != _sha(_json_bytes(body)):
        _fail("invalid_index", "Payload checksum mismatch")
    return value


def _local(root, relative):
    """Reject escapes/symlinks even when a path does not exist yet."""
    if not isinstance(relative, str) or not relative or len(relative) > MAX_PATH:
        _fail("invalid_path", "Expected a bounded repository-relative path")
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts or "\\" in relative or any(ord(c) < 32 for c in relative):
        _fail("invalid_path", f"Unsafe local path: {relative!r}")
    current = root
    for part in path.parts:
        current /= part
        if current.is_symlink():
            _fail("invalid_path", f"Symlink is unsupported: {relative}")
    if not current.resolve().is_relative_to(root):
        _fail("invalid_path", f"Path leaves checkout: {relative}")
    return current


def _relative(root, value):
    path = Path(value)
    if path.is_absolute():
        try:
            value = path.relative_to(root).as_posix()
        except ValueError:
            _fail("invalid_path", "Paths must be within --root")
    return _local(root, str(value)).relative_to(root).as_posix()


def _read(path, limit=MAX_FILE_BYTES):
    try:
        if not stat.S_ISREG(path.stat().st_mode):
            _fail("invalid_path", f"Not a regular file: {path.name}")
        with path.open("rb") as stream:
            raw = stream.read(limit + 1)
    except FileNotFoundError:
        _fail("missing_input", f"Required file is absent: {path}")
    if len(raw) > limit:
        _fail("capacity", f"File exceeds {limit} bytes: {path.name}")
    return raw


def _walk(root, directory):
    """Deterministic bounded traversal; never materialize an unbounded rglob."""
    count = 0

    def visit(path, depth):
        nonlocal count
        if depth > MAX_DEPTH:
            _fail("capacity", "Directory depth exceeded")
        with os.scandir(path) as entries:
            children = []
            for entry in entries:
                count += 1
                if count > MAX_FILES:
                    _fail("capacity", "Directory entry limit exceeded")
                children.append(entry)
        for entry in sorted(children, key=lambda item: item.name):
            candidate = Path(entry.path)
            relative = candidate.relative_to(root).as_posix()
            _local(root, relative)
            if entry.is_dir(follow_symlinks=False):
                if entry.name not in SKIP_DIRS:
                    yield from visit(candidate, depth + 1)
            elif entry.is_file(follow_symlinks=False):
                yield relative
            else:
                _fail("invalid_path", f"Not a regular input: {relative}")

    start = _local(root, directory)
    if start.exists():
        yield from visit(start, 0)


def _hash_files(root, paths):
    result = {}
    total = 0
    for relative in paths:
        if len(result) >= MAX_FILES:
            _fail("capacity", "File count exceeded")
        raw = _read(_local(root, relative))
        total += len(raw)
        if total > MAX_TOTAL_BYTES:
            _fail("capacity", "Total input byte limit exceeded")
        result[relative] = _sha(raw)
    return dict(sorted(result.items()))


def _inputs(root):
    paths = [name for name in ROOT_INPUTS if _local(root, name).exists()]
    if "Cargo.toml" not in paths:
        _fail("missing_input", "Cargo.toml is absent")
    if _local(root, "scripts/api-index.py").exists():
        paths.append("scripts/api-index.py")
    for directory in INPUT_DIRS:
        paths.extend(_walk(root, directory))
        if len(paths) > MAX_FILES:
            _fail("capacity", "Input file count exceeded")
    return _hash_files(root, sorted(paths))


def _packages(root, names):
    workspace = tomllib.loads(_read(root / "Cargo.toml").decode()).get("workspace")
    if not isinstance(workspace, dict):
        _fail("invalid_workspace", "Cargo.toml must declare a workspace")
    members = workspace.get("members", [])
    if not isinstance(members, list) or len(members) > MAX_FILES:
        _fail("invalid_workspace", "Expected bounded workspace.members")
    packages = []
    for member in members:
        if not isinstance(member, str) or not member or any(char in member for char in "*?["):
            _fail("invalid_workspace", "Only explicit workspace member paths are supported")
        if Path(member).parts[0] not in ("crates", "examples"):
            _fail("invalid_workspace", "Workspace member must be under crates/ or examples/")
        manifest = _local(root, member + "/Cargo.toml")
        data = tomllib.loads(_read(manifest).decode())
        package = data.get("package")
        if not isinstance(package, dict) or not isinstance(package.get("name"), str) or not package["name"]:
            _fail("invalid_workspace", "Invalid package.name")
        name = package["name"]
        if not isinstance(data.get("lib", {}), dict):
            _fail("invalid_workspace", "Invalid lib target")
        if "lib" not in data and not (manifest.parent / "src/lib.rs").is_file():
            if name in names:
                _fail("invalid_workspace", f"Only library targets are supported: {name}")
            continue
        crate = data.get("lib", {}).get("name", name.replace("-", "_"))
        if not _symbol(crate) or "::" in crate:
            _fail("invalid_workspace", "Invalid library crate name")
        packages.append({"package": name, "crate": crate, "manifest": member + "/Cargo.toml"})
    if len({p["crate"] for p in packages}) != len(packages):
        _fail("invalid_workspace", "Duplicate crate names")
    missing = set(names) - {p["package"] for p in packages}
    if missing:
        _fail("unknown_package", "Unknown packages: " + ", ".join(sorted(missing)))
    return sorted((p for p in packages if not names or p["package"] in names), key=lambda p: p["crate"])


def _symbol(text):
    return isinstance(text, str) and bool(text) and len(text) <= MAX_PATH and all(
        part.removeprefix("r#").isidentifier() for part in text.split("::"))


def prepare(root, *, packages=(), features=(), all_features=False, no_default_features=False, target=None):
    """Return a sealed input snapshot; performs no builds or writes.

    ``features`` are caller-declared Cargo feature arguments, not requirements
    inferred for individual symbols. Reuse this snapshot only for unchanged
    inputs and HTML generated after it. See the module's executable CLI recipe.
    """
    root = Path(root).resolve()
    selected = _packages(root, packages)
    if not selected:
        _fail("invalid_workspace", "No selected workspace packages")
    if len(features) > 128 or any(not isinstance(f, str) or not f or len(f) > MAX_PATH for f in features):
        _fail("invalid_arguments", "Feature arguments exceed bounds")
    if target is not None and (not isinstance(target, str) or not target or len(target) > MAX_PATH):
        _fail("invalid_arguments", "Invalid target")
    return _sealed({"schema_version": SCHEMA_VERSION, "kind": "ferravex.api-inputs",
                    "inputs": _inputs(root), "packages": selected,
                    "configuration": {"features": sorted(set(features)), "all_features": bool(all_features),
                                      "no_default_features": bool(no_default_features), "target": target,
                                      "evidence": "caller-declared"},
                    "prepared_ns": time.time_ns()})


class _Rustdoc(HTMLParser):
    """Capture only Rustdoc declarations; filenames never supply symbol names."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []
        self.title = []
        self.heading = []
        self.top = self._record(0)
        self.member = None
        self.members = []
        self.generator = None
        self.version = None
        self.description = ""
        self.has_main = False
        self.source_text = []
        self.source_mode = False

    @staticmethod
    def _record(depth):
        return {"depth": depth, "anchor": "", "name": [], "signature": [], "source": None, "availability": []}

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if tag == "meta":
            if attrs.get("name") == "generator":
                self.generator = attrs.get("content")
            if attrs.get("name") == "rustdoc-vars":
                self.version = attrs.get("data-rustdoc-version")
            if attrs.get("name") == "description":
                self.description = attrs.get("content", "")
                self.source_mode = self.description.startswith("Source of the Rust file `")
        if tag == "br":
            self.handle_data(" ")
        if tag in VOID_TAGS:
            return
        classes = attrs.get("class", "").split()
        ident = attrs.get("id", "")
        skip = (any(frame[2] for frame in self.stack) or tag in ("script", "style", "nav", "button")
                or "data-nosnippet" in attrs or "anchor" in classes or "hideme" in classes
                or "docblock" in classes or "trait-impl" in classes
                or ("implementors-toggle" in classes and _compact("".join(self.heading)).startswith("Trait "))
                or ident.startswith(("trait-implementations-list", "blanket-implementations-list",
                                     "synthetic-implementations-list", "implementors-list", "foreign-impls")))
        frame = [tag, attrs, skip, None]
        self.stack.append(frame)
        depth = len(self.stack)
        if depth > MAX_DEPTH:
            _fail("capacity", "HTML nesting limit exceeded")
        if skip:
            return
        if tag == "title":
            frame[3] = self.title
        if self.source_mode:
            if tag == "code" and any(t == "pre" and "rust" in a.get("class", "").split() for t, a, _, _ in self.stack):
                frame[3] = self.source_text
            return
        in_main = any(a.get("id") == "main-content" for _, a, _, _ in self.stack)
        if not in_main:
            return
        self.has_main = True
        main_heading = any("main-heading" in a.get("class", "").split() for _, a, _, _ in self.stack)
        if tag == "h1" and main_heading:
            frame[3] = self.heading
        inherent = any(a.get("id") == "implementations-list" for _, a, _, _ in self.stack)
        eligible = inherent or _compact("".join(self.heading)).startswith("Trait ")
        member_anchor = tag == "section" and ident.partition(".")[0] in MEMBER_KINDS
        if eligible and self.member is None and ((tag == "details" and "method-toggle" in classes) or member_anchor):
            self.member = self._record(depth)
        record = self.member or self.top
        if self.member and member_anchor:
            record["anchor"] = ident
        if tag == "a" and "src" in classes and (self.member or main_heading):
            record["source"] = attrs.get("href")
        if tag == "pre" and "item-decl" in classes:
            frame[3] = self.top["signature"]
        if self.member and tag == "h4" and "code-header" in classes:
            frame[3] = record["signature"]
        if self.member and tag == "a" and attrs.get("href") == "#" + record["anchor"]:
            if any(t == "h4" for t, _, _, _ in self.stack):
                frame[3] = record["name"]
        if "portability" in classes and "stab" in classes:
            frame[3] = record["availability"]

    def handle_endtag(self, tag):
        match = next((i for i in range(len(self.stack) - 1, -1, -1) if self.stack[i][0] == tag), None)
        if match is None:
            return
        if self.member and self.member["depth"] > match:
            self.members.append(self.member)
            if len(self.members) > MAX_SYMBOLS:
                _fail("capacity", "Member count exceeded")
            self.member = None
        del self.stack[match:]

    def handle_data(self, data):
        if any(frame[2] for frame in self.stack):
            return
        for _, _, _, capture in self.stack:
            if capture is not None:
                capture.append(data)


def _parse(raw):
    parser = _Rustdoc()
    parser.feed(raw.decode("utf-8"))
    parser.close()
    return parser


def _identity(parser):
    if parser.generator != "rustdoc" or not parser.has_main:
        return None
    heading = _compact("".join(parser.heading))
    kind = next((kind for label, kind in KINDS.items() if heading.startswith(label + " ")), None)
    if kind is None:
        return None
    title = _compact("".join(parser.title))
    if not title.endswith(" - Rust"):
        _fail("invalid_evidence", "Unsupported Rustdoc title")
    title = title.removesuffix(" - Rust")
    name, separator, context = title.partition(" in ")
    qualified = context + "::" + name if separator else name
    if not _symbol(qualified):
        _fail("invalid_evidence", "Invalid Rustdoc qualified name")
    label = next(label for label, value in KINDS.items() if value == kind)
    if heading[len(label) + 1:] != qualified.split("::")[-1]:
        _fail("invalid_evidence", "Rustdoc title and heading disagree")
    return qualified, kind


def _doc_inventory(root, docs, packages):
    paths = []
    for package in packages:
        directory = docs + "/" + package["crate"]
        if not _local(root, directory + "/index.html").is_file():
            _fail("missing_index", f"Missing crate Rustdoc: {directory}/index.html")
        paths.extend(path for path in _walk(root, directory) if path.endswith(".html"))
        if len(paths) > MAX_FILES:
            _fail("capacity", "Rustdoc page count exceeded")
    return sorted(paths)


def _verify_snapshot(root, snapshot):
    _checked_envelope(snapshot, "ferravex.api-inputs")
    if type(snapshot.get("prepared_ns")) is not int or not 0 < snapshot["prepared_ns"] < 2**63:
        _fail("invalid_index", "Invalid preparation timestamp")
    _validate_hashes(root, snapshot.get("inputs"))
    packages = snapshot.get("packages")
    if not isinstance(packages, list) or not 0 < len(packages) <= MAX_FILES:
        _fail("invalid_index", "Invalid package list")
    for package in packages:
        if (not isinstance(package, dict) or set(package) != {"package", "crate", "manifest"}
                or any(not isinstance(value, str) or not 0 < len(value) <= MAX_PATH for value in package.values())):
            _fail("invalid_index", "Invalid package descriptor")
    config = snapshot.get("configuration")
    if not isinstance(config, dict) or set(config) != {"features", "all_features", "no_default_features", "target", "evidence"}:
        _fail("invalid_index", "Invalid build configuration")
    features = config["features"]
    if (not isinstance(features, list) or len(features) > 128
            or any(not isinstance(f, str) or not 0 < len(f) <= MAX_PATH for f in features)
            or features != sorted(set(features)) or config["evidence"] != "caller-declared"
            or type(config["all_features"]) is not bool or type(config["no_default_features"]) is not bool
            or (config["target"] is not None and (not isinstance(config["target"], str) or not 0 < len(config["target"]) <= MAX_PATH))):
        _fail("invalid_index", "Invalid feature/target configuration")
    if _inputs(root) != snapshot.get("inputs"):
        _fail("stale_index", "Build inputs changed; prepare a new snapshot and regenerate Rustdoc")
    expected = _packages(root, [p["package"] for p in snapshot["packages"]])
    if expected != snapshot["packages"]:
        _fail("invalid_index", "Snapshot packages disagree with workspace")


def _validate_hashes(root, hashes):
    if not isinstance(hashes, dict) or not 0 < len(hashes) <= MAX_FILES:
        _fail("invalid_index", "Invalid file hash map")
    for path, digest in hashes.items():
        _local(root, path)
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            _fail("invalid_index", "Invalid SHA-256 digest")


def _validate_symbols(root, data):
    """Check serialized bounds, even for a checksum-correct foreign producer."""
    symbols = data.get("symbols")
    if not isinstance(symbols, list) or not 0 < len(symbols) <= MAX_SYMBOLS:
        _fail("invalid_index", "Invalid symbol count")
    seen = set()
    for item in symbols:
        if not isinstance(item, dict) or set(item) != {"name", "qualified_path", "import_path", "kind", "signature",
                                                      "signature_truncated", "doc", "source", "features", "provenance"}:
            _fail("invalid_index", "Invalid symbol record")
        name, qualified, kind = item.get("name"), item.get("qualified_path"), item.get("kind")
        if (not _symbol(qualified) or name != qualified.split("::")[-1]
                or not isinstance(kind, str) or kind not in set(KINDS.values()) | set(MEMBER_KINDS.values())):
            _fail("invalid_index", "Invalid qualified symbol/kind")
        expected_import = qualified.rsplit("::", 1)[0] if kind in MEMBER_KINDS.values() else qualified
        if item.get("import_path") != expected_import or (qualified, kind) in seen:
            _fail("invalid_index", "Invalid import path or duplicate symbol")
        seen.add((qualified, kind))
        signature = item.get("signature")
        if ((signature is None and kind not in ("module", "crate"))
                or (signature is not None and (not isinstance(signature, str) or not 0 < len(signature) <= MAX_SIGNATURE))
                or type(item.get("signature_truncated")) is not bool):
            _fail("invalid_index", "Invalid bounded signature")
        doc, source = item.get("doc"), item.get("source")
        if (not isinstance(doc, dict) or set(doc) != {"path", "fragment"} or not isinstance(source, dict)
                or set(source) != {"path", "line_start", "line_end", "doc_path", "fragment"}):
            _fail("invalid_index", "Invalid symbol locations")
        for location, key in ((doc, "path"), (source, "doc_path")):
            path = location.get(key)
            _local(root, path)
            fragment = location.get("fragment")
            if path not in data["artifacts"] or (fragment is not None and (not isinstance(fragment, str) or len(fragment) > MAX_PATH)):
                _fail("invalid_index", "Untracked symbol location")
        _local(root, source.get("path"))
        if source["path"] not in data["snapshot"]["inputs"]:
            _fail("invalid_index", "Untracked source path")
        for key in ("line_start", "line_end"):
            if source.get(key) is not None and (type(source[key]) is not int or not 0 < source[key] <= MAX_FILE_BYTES):
                _fail("invalid_index", "Invalid source line")
        features = item.get("features")
        if (not isinstance(features, dict) or set(features) != {"configuration", "availability", "availability_truncated", "requirements_known"}
                or features.get("configuration") != "snapshot.configuration"):
            _fail("invalid_index", "Invalid symbol feature provenance")
        availability = features.get("availability")
        if (features.get("requirements_known") is not False or type(features.get("availability_truncated")) is not bool
                or (availability is not None and (not isinstance(availability, str) or len(availability) > MAX_AVAILABILITY))):
            _fail("invalid_index", "Invalid bounded availability")
        provenance = item.get("provenance")
        if (not isinstance(provenance, dict) or set(provenance) != {"format", "identity", "page_sha256"}
                or provenance.get("format") != "rustdoc-html"
                or provenance.get("identity") not in ("title+main-heading", "member-heading+anchor")
                or provenance.get("page_sha256") != data["artifacts"][doc["path"]]):
            _fail("invalid_index", "Invalid symbol provenance")


def build_index(root, docs_root, snapshot):
    """Return a sealed index of generated declarations, or raise IndexError.

    Reads generated Rustdoc and validates the ``prepare`` snapshot; does not
    generate documentation, execute examples, or write files.
    """
    root = Path(root).resolve()
    docs = _relative(root, docs_root)
    _verify_snapshot(root, snapshot)
    inventory = _doc_inventory(root, docs, snapshot["packages"])
    artifacts = {}
    source_cache = {}
    symbols = []
    versions = set()
    total = 0
    skipped = 0

    def evidence(relative):
        nonlocal total
        path = _local(root, relative)
        raw = _read(path)
        if relative not in artifacts:
            total += len(raw)
            if total > MAX_TOTAL_BYTES or len(artifacts) >= MAX_FILES:
                _fail("capacity", "Rustdoc evidence limit exceeded")
        if path.stat().st_mtime_ns < snapshot["prepared_ns"]:
            _fail("stale_index", f"Rustdoc predates the input snapshot: {relative}")
        artifacts[relative] = _sha(raw)
        return raw

    def source_location(page, href):
        if not href:
            return None
        url = urlsplit(href)
        if url.scheme or url.netloc or url.query:
            return None
        # Rustdoc relative links legitimately contain '..'; resolve them before
        # applying the stricter serialized-path validator.
        source = (root / page).parent / unquote(url.path)
        resolved = source.resolve()
        if not resolved.is_relative_to(root / docs / "src"):
            _fail("invalid_evidence", f"Source link leaves Rustdoc src/: {page}")
        relative = resolved.relative_to(root).as_posix()
        _local(root, relative)
        if relative not in source_cache:
            parsed = _parse(evidence(relative))
            match = re.fullmatch(r"Source of the Rust file `([^`]+)`\.", parsed.description)
            if parsed.generator != "rustdoc" or not match or not parsed.source_text:
                _fail("invalid_evidence", f"Unsupported rendered source: {relative}")
            source_path = match[1]
            if source_path not in snapshot["inputs"]:
                _fail("invalid_evidence", f"Source outside snapshotted inputs: {source_path}")
            actual = _read(_local(root, source_path)).decode("utf-8").replace("\r\n", "\n")
            rendered = "".join(parsed.source_text).replace("\r\n", "\n")
            if actual != rendered:
                _fail("stale_index", f"Rendered source differs from checkout: {source_path}")
            source_cache[relative] = (source_path, len(actual.splitlines()))
        line = re.fullmatch(r"([1-9][0-9]*)(?:-([1-9][0-9]*))?", url.fragment)
        if url.fragment and not line:
            _fail("invalid_evidence", "Unsupported source line fragment")
        first = int(line[1]) if line else None
        last = int(line[2] or line[1]) if line else None
        source_path, lines = source_cache[relative]
        if first is not None and (last < first or last > lines):
            _fail("invalid_evidence", "Invalid source line range")
        return {"path": source_path, "line_start": first, "line_end": last,
                "doc_path": relative, "fragment": url.fragment or None}

    def append(qualified, kind, record, page, owner=None):
        if len(symbols) >= MAX_SYMBOLS:
            _fail("capacity", "Symbol count exceeded")
        signature = _compact("".join(record["signature"]))
        availability = _compact("".join(record["availability"]))
        if not signature and kind not in ("crate", "module"):
            _fail("invalid_evidence", f"Missing rendered signature: {qualified}")
        source = source_location(page, record["source"])
        if source is None:
            return False  # External/no defining evidence is not a first-party API.
        symbols.append({"name": qualified.split("::")[-1], "qualified_path": qualified,
                        "import_path": owner or qualified, "kind": kind,
                        "signature": signature[:MAX_SIGNATURE] or None,
                        "signature_truncated": len(signature) > MAX_SIGNATURE,
                        "doc": {"path": page, "fragment": record["anchor"] or None},
                        "source": source,
                        "features": {"configuration": "snapshot.configuration",
                                     "availability": availability[:MAX_AVAILABILITY] or None,
                                     "availability_truncated": len(availability) > MAX_AVAILABILITY,
                                     "requirements_known": False},
                        "provenance": {"format": "rustdoc-html", "page_sha256": artifacts[page],
                                       "identity": "title+main-heading" if owner is None else "member-heading+anchor"}})
        return True

    for page in inventory:
        parser = _parse(evidence(page))
        identity = _identity(parser)
        if identity is None:
            skipped += 1
            continue
        qualified, kind = identity
        crate = Path(page).relative_to(docs).parts[0]
        if qualified.split("::")[0] != crate:
            _fail("invalid_evidence", f"Documented crate disagrees with selected package: {page}")
        if parser.version:
            versions.add(parser.version)
        if not append(qualified, kind, parser.top, page):
            skipped += 1
            continue
        for member in parser.members:
            anchor_kind, _, anchor_name = member["anchor"].partition(".")
            name = _compact("".join(member["name"]))
            if anchor_kind not in MEMBER_KINDS or not _symbol(name) or "::" in name or name != anchor_name:
                _fail("invalid_evidence", f"Member anchor and declaration disagree: {page}#{member['anchor']}")
            append(qualified + "::" + name, MEMBER_KINDS[anchor_kind], member, page, owner=qualified)
    if not symbols:
        _fail("missing_index", "No supported first-party Rustdoc symbols found")
    symbols.sort(key=lambda item: (item["qualified_path"], item["kind"], item["doc"]["path"], item["doc"]["fragment"] or ""))
    identities = [(s["qualified_path"], s["kind"]) for s in symbols]
    if len(set(identities)) != len(identities):
        _fail("invalid_evidence", "Duplicate qualified symbols of the same kind")
    # Catch changes during parsing; never publish an index over mixed inputs.
    _verify_snapshot(root, snapshot)
    if inventory != _doc_inventory(root, docs, snapshot["packages"]) or artifacts != _hash_files(root, sorted(artifacts)):
        _fail("stale_index", "Rustdoc changed while indexing")
    return _sealed({"schema_version": SCHEMA_VERSION, "kind": "ferravex.api-index",
                    "snapshot": snapshot, "docs_root": docs, "inventory": inventory,
                    "artifacts": dict(sorted(artifacts.items())), "symbols": symbols,
                    "rustdoc_versions": sorted(versions), "skipped_pages": skipped})


def _normalize(value):
    value = re.sub(r"\s*::\s*", "::", _compact(value).strip("`")).removeprefix("::")
    value = value.removesuffix("()").removesuffix("!")
    return "::".join(part.removeprefix("r#") for part in value.split("::")).casefold()


def _distance(left, right, bound=2):
    """Banded Levenshtein with a fixed work/score ceiling; no fuzzy prose."""
    if abs(len(left) - len(right)) > bound:
        return bound + 1
    previous = {i: i for i in range(min(len(right), bound) + 1)}
    for i, char in enumerate(left, 1):
        row = {0: i} if i <= bound else {}
        for j in range(max(1, i - bound), min(len(right), i + bound) + 1):
            row[j] = min(row.get(j - 1, bound + 1) + 1, previous.get(j, bound + 1) + 1,
                         previous.get(j - 1, bound + 1) + (char != right[j - 1]))
        if not row or min(row.values()) > bound:
            return bound + 1
        previous = row
    return previous.get(len(right), bound + 1)


def search_index(root, index, query, *, mode="auto", limit=10):
    """Validate an index on disk and return a deterministic bounded JSON report.

    Modes: exact = case-insensitive component-suffix equality; fuzzy = exact,
    last-component prefixes, then edit distance <=2 (<=1 for short names); auto
    uses exact matches exclusively when any exist, otherwise fuzzy. Ranking
    ties use qualified path/kind/doc location. Associated imports name owners.
    A valid zero-match query succeeds; stale/missing evidence always raises.
    """
    if not isinstance(query, str) or not query.strip() or len(query) > MAX_QUERY:
        _fail("invalid_arguments", f"Query must contain 1..{MAX_QUERY} characters")
    term = _normalize(query)
    if not _symbol(term) or len(term) > MAX_QUERY:
        _fail("invalid_arguments", "Query must be a symbol name or qualified path")
    if mode not in ("auto", "exact", "fuzzy") or type(limit) is not int or not 1 <= limit <= MAX_RESULTS:
        _fail("invalid_arguments", "Invalid search mode or limit (1..100)")
    root = Path(root).resolve()
    path = _local(root, _relative(root, index))
    if not path.is_file():
        _fail("missing_index", f"API index is absent: {path}")
    data = _checked_envelope(_load(path), "ferravex.api-index")
    _verify_snapshot(root, data["snapshot"])
    _validate_hashes(root, data.get("artifacts"))
    _validate_symbols(root, data)
    _local(root, data.get("docs_root"))
    stored_inventory = data.get("inventory")
    if (not isinstance(stored_inventory, list) or not 0 < len(stored_inventory) <= MAX_FILES
            or any(not isinstance(path, str) or path not in data["artifacts"] for path in stored_inventory)
            or stored_inventory != sorted(set(stored_inventory))):
        _fail("invalid_index", "Invalid page inventory")
    versions = data.get("rustdoc_versions")
    if not isinstance(versions, list) or len(versions) > MAX_FILES or any(not isinstance(v, str) or len(v) > MAX_PATH for v in versions):
        _fail("invalid_index", "Invalid Rustdoc version metadata")
    inventory = _doc_inventory(root, data["docs_root"], data["snapshot"]["packages"])
    if inventory != data["inventory"] or _hash_files(root, sorted(data["artifacts"])) != data["artifacts"]:
        _fail("stale_index", "Rustdoc artifacts changed; regenerate the index")
    symbols = data["symbols"]
    exact = []
    fuzzy = []
    exact_count = fuzzy_count = 0
    for item in symbols:
        qualified = item["qualified_path"]
        if not _symbol(qualified):
            _fail("invalid_index", "Invalid qualified symbol")
        parts = _normalize(qualified).split("::")
        # Only the same number of path components can be compared: fuzzy name
        # lookup cannot turn a short type name into one of its members.
        wanted = term.count("::") + 1
        if len(parts) < wanted:
            continue
        alias = "::".join(parts[-wanted:])
        if alias == term:
            rank, match = 0, "exact"
        elif mode == "exact":
            continue
        elif alias.startswith(term):
            rank, match = 1, "prefix"
        else:
            distance = _distance(term, alias, 1 if len(term) < 5 else 2)
            if distance > (1 if len(term) < 5 else 2):
                continue
            rank, match = 1 + distance, "edit"
        key = (rank, qualified, item["kind"], item["doc"]["path"], item["doc"]["fragment"] or "")
        if match == "exact":
            exact_count += 1
            selected = exact
        else:
            fuzzy_count += 1
            selected = fuzzy
        selected.append((key, match, item))
        selected.sort(key=lambda candidate: candidate[0])
        del selected[limit:]
    if mode == "exact" or (mode == "auto" and exact_count):
        chosen, count = exact, exact_count
    else:
        chosen = sorted(exact + fuzzy, key=lambda candidate: candidate[0])[:limit]
        count = exact_count + fuzzy_count
    results = []
    for _, match, item in chosen:
        result = dict(item, match=match)
        result["features"] = dict(item["features"], configuration=data["snapshot"]["configuration"])
        result["doc"] = dict(item["doc"], url=_file_url(root, item["doc"]["path"], item["doc"]["fragment"]))
        result["source"] = dict(item["source"], url=_file_url(root, item["source"]["doc_path"], item["source"]["fragment"]))
        results.append(result)
    return {"schema_version": SCHEMA_VERSION, "status": "ok", "query": query, "mode": mode,
            "total_matches": count, "truncated": count > limit, "results": results,
            "provenance": {"index_sha256": data["payload_sha256"], "docs_root": data["docs_root"],
                           "snapshot_sha256": data["snapshot"]["payload_sha256"],
                           "rustdoc_versions": data["rustdoc_versions"], "freshness": "verified"}}


def _file_url(root, path, fragment):
    return _local(root, path).as_uri() + ("#" + quote(fragment, safe=".-_") if fragment else "")


def _load(path):
    try:
        return json.loads(_read(path, MAX_JSON_BYTES))
    except (ValueError, RecursionError) as error:
        if isinstance(error, IndexError):
            raise
        _fail("invalid_index", "Malformed JSON input")


def _write_new(path, value):
    raw = _json_bytes(value)
    if len(raw) > MAX_JSON_BYTES:
        _fail("capacity", "JSON output exceeds byte limit")
    try:
        with path.open("xb") as stream:
            stream.write(raw)
    except FileExistsError:
        _fail("output_exists", f"Output already exists: {path}")


class _ArgumentParser(argparse.ArgumentParser):
    def error(self, message):
        _fail("invalid_arguments", message)


def main(argv=None):
    """Run the CLI; return 0 on success, 2 on errors. ``--help`` is human text."""
    try:
        parser = _ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
        parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
        commands = parser.add_subparsers(dest="command", required=True)
        snapshot = commands.add_parser("prepare", help="Snapshot inputs BEFORE caller generates Rustdoc")
        snapshot.add_argument("-p", "--package", action="append", default=[])
        snapshot.add_argument("--features", action="append", default=[])
        snapshot.add_argument("--all-features", action="store_true")
        snapshot.add_argument("--no-default-features", action="store_true")
        snapshot.add_argument("--target")
        snapshot.add_argument("--output", required=True)
        build = commands.add_parser("build", help="Index existing Rustdoc against an earlier input snapshot")
        build.add_argument("--snapshot", required=True)
        build.add_argument("--docs", default="target/doc")
        build.add_argument("--output", default="target/api-index.json")
        search = commands.add_parser("search", help="Read-only fresh symbol lookup")
        search.add_argument("query")
        search.add_argument("--index", default="target/api-index.json")
        search.add_argument("--mode", choices=("auto", "exact", "fuzzy"), default="auto")
        search.add_argument("--limit", type=int, default=10)
        args = parser.parse_args(argv)
        root = args.root.resolve()
        if args.command == "search":
            report = search_index(root, args.index, args.query, mode=args.mode, limit=args.limit)
        else:
            output = _local(root, _relative(root, args.output))
            if output.exists():
                _fail("output_exists", f"Output already exists: {output}")
            if not output.parent.is_dir():
                _fail("missing_input", "Output parent directory must exist")
            # Writing into input scope would immediately invalidate the snapshot.
            relative = output.relative_to(root).as_posix()
            if relative.split("/")[0] in INPUT_DIRS or relative in ROOT_INPUTS:
                _fail("invalid_path", "Output must be outside snapshotted input directories")
            if args.command == "prepare":
                data = prepare(root, packages=args.package, features=args.features, all_features=args.all_features,
                               no_default_features=args.no_default_features, target=args.target)
            else:
                snapshot_path = _local(root, _relative(root, args.snapshot))
                data = build_index(root, args.docs, _load(snapshot_path))
            _write_new(output, data)
            report = {"schema_version": SCHEMA_VERSION, "status": "ok", "command": args.command,
                      "output": relative, "payload_sha256": data["payload_sha256"]}
            if args.command == "build":
                report.update(symbols=len(data["symbols"]), skipped_pages=data["skipped_pages"])
        print(_json_bytes(report).decode(), end="")
        return 0
    except IndexError as error:
        report = {"schema_version": SCHEMA_VERSION, "status": "error", "error": {"code": error.code, "message": str(error)}}
    except (OSError, UnicodeError, ValueError, KeyError, TypeError, RecursionError) as error:
        report = {"schema_version": SCHEMA_VERSION, "status": "error", "error": {"code": "invalid_input", "message": str(error)}}
    print(_json_bytes(report).decode(), end="")
    return 2


if __name__ == "__main__":
    sys.exit(main())
