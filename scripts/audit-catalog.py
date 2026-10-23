#!/usr/bin/env python3
"""Audit catalog metadata, capability source anchors and session migration.

Examples::

    python3 -B scripts/audit-catalog.py --json
    python3 -B scripts/audit-catalog.py --migration --output target/catalog.json
    python3 -B scripts/audit-catalog.py --require-session
    python3 -B scripts/audit-catalog.py --capabilities-schema

Each catalog entry may add a ``capabilities`` object with ``schema_version: 1``
and any of controls, pause, replay, content and presentation. Each group maps
named features to ``{evidence: [{path, snippet}], note?: str}``. The schema CLI
documents the allowed features and bounds. Existing catalog fields/IDs and the
audit report's v1 envelope are unchanged; the extension has its own version.
Omission means unreported, never unsupported. Notes qualify modes/features;
multiple pause features may coexist (offline simulation vs. a network menu).

Evidence snippets are contiguous Rust tokens from registered example sources,
insensitive to whitespace/comments, excluding literals and test-only items.
They detect stale declarations, not semantics, device support or reachability.
Use a concrete call/binding rather than a bare identifier/import. Shared host
capabilities must not be inferred just because the engine implements them.
``source_matched`` and migration ``active`` are STATIC evidence only: neither
establishes compilation, feature availability, runtime correctness or playtests.

The migration report is static evidence, not a Rust compiler or a gameplay test.
It recognizes imports (including groups/aliases), GameSession new/try_new/
from_config calls, typed receivers and calls, ignoring comments, literals and
test-only items. SessionConfig alone is not a session. Target-specific code
is included; macro expansion, full name resolution and runtime reachability are
not evaluated. In particular, an externally clocked network host need not call
GameSession.advance to use the session lifecycle correctly.
"""
import argparse
from collections import Counter
from html.parser import HTMLParser
import json
from pathlib import Path, PurePosixPath
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
IDENTIFIER = re.compile(r"[a-z][a-z0-9]*(?:-[a-z0-9]+)*\Z")
SESSION_TYPES = {
    ("ferravex_core", "session", "GameSession"),
    ("ferravex", "core", "session", "GameSession"),
    ("ferravex", "prelude", "GameSession"),
}
SESSION_CONSTRUCTORS = {"new", "try_new", "from_config"}
SESSION_METHODS = {
    "state", "is_started", "is_playing", "is_paused", "start", "restart",
    "reset", "set_paused", "toggle_pause", "finish", "advance", "tick", "dt",
}
RAW_STRING = re.compile(r'(?:br|cr|r)(\#*)"')
QUOTED = re.compile(r'''(?:b|c)?"(?:\\.|[^"\\])*"|b?'(?:\\(?:u\{[^}]*\}|x..|.)|[^'\\])' ''', re.S | re.X)
TOKEN = re.compile(r"[A-Za-z_][A-Za-z_0-9]*|::|\S")
CAPABILITY_FEATURES = {
    "controls": {
        "action_map": "Game code configures a shared action map or PlayerInput profile.",
        "keyboard": "Game code declares or consumes keyboard input.",
        "pointer": "Game code declares or consumes mouse/pointer input.",
        "gamepad": "Game code declares or consumes gamepad input; no device certification.",
        "touch": "Game code configures or consumes touch input; no device certification.",
        "rebinding": "Game code exposes input rebinding; engine availability alone is insufficient.",
    },
    "pause": {
        "simulation": "An offline/local simulation can pause; qualify modes in note.",
        "local_menu": "A local menu suppresses commands while network authority continues.",
        "single_step": "The game offers an explicit simulation step while paused.",
    },
    "replay": {
        "record": "Game code records simulation data; no determinism guarantee.",
        "playback": "Game code consumes recorded data for playback.",
        "ghost": "Game code builds or displays a ghost track.",
        "import": "Game code accepts serialized replay data.",
        "export": "Game code exposes serialized replay data.",
    },
    "content": {
        "authored_scene": "Game code loads an authored scene.",
        "scene_editor": "Game code connects to an editing interface; qualify build features.",
        "hot_reload": "Game code polls and applies changed content.",
    },
    "presentation": {
        "feedback": "Game code presents feedback from game events.",
        "audio": "Game code triggers audio cues; no audio-device verification.",
        "interpolation": "Game code interpolates simulation poses for presentation.",
        "reduced_motion": "Game code applies a reduced-motion preference.",
        "reduced_flash": "Game code applies a reduced-flash preference.",
    },
}
CAPABILITY_NOTE = (
    "Static source anchors only, including target/feature-conditional code. "
    "Omitted features are unreported. Matching anchors do not prove semantics, "
    "compilation, runtime support, device support or determinism. No macro/include!/"
    "custom #[path] expansion, full name resolution or playtest was performed."
)


def capabilities_schema():
    """Return a fresh JSON Schema for the optional catalog capabilities object.

    >>> capabilities_schema()['properties']['schema_version']['const']
    1

    Path registration and token matching are additional source checks performed
    by :func:`capability_usage`, not claims made by JSON Schema validation.
    """
    def string(limit):
        return {"type": "string", "minLength": 1, "maxLength": limit, "pattern": r"\S"}

    def claim(description):
        return {
            "type": "object", "additionalProperties": False,
            "description": description, "required": ["evidence"],
            "properties": {
                "note": string(400),
                "evidence": {
                    "type": "array", "minItems": 1, "maxItems": 4, "uniqueItems": True,
                    "items": {
                        "type": "object", "additionalProperties": False,
                        "required": ["path", "snippet"],
                        "properties": {"path": string(240), "snippet": string(256)},
                    },
                },
            },
        }

    properties = {"schema_version": {"type": "integer", "const": 1}}
    for group, features in CAPABILITY_FEATURES.items():
        properties[group] = {
            "type": "object", "additionalProperties": False, "minProperties": 1,
            "properties": {name: claim(description) for name, description in features.items()},
        }
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Ferravex catalog capabilities v1", "description": CAPABILITY_NOTE,
        "type": "object", "additionalProperties": False, "minProperties": 2,
        "required": ["schema_version"], "properties": properties,
    }


def _schema_errors(value, schema, location="capabilities"):
    """Validate the closed subset of JSON Schema emitted above, using stdlib."""
    expected = schema["type"]
    types = {"object": dict, "array": list, "string": str, "integer": int}
    integer_number = expected == "integer" and type(value) is float and value.is_integer()
    if type(value) is not types[expected] and not integer_number:
        yield f"{location}: expected {expected}"
        return
    if "const" in schema and value != schema["const"]:
        yield f"{location}: expected {schema['const']}"
    if expected == "object":
        if len(value) < schema.get("minProperties", 0):
            yield f"{location}: expected at least {schema['minProperties']} properties"
        for name in schema.get("required", []):
            if name not in value:
                yield f"{location}.{name}: required"
        for name, child in value.items():
            if name not in schema["properties"]:
                yield f"{location}.{name}: unknown field"
            else:
                yield from _schema_errors(child, schema["properties"][name], f"{location}.{name}")
    elif expected == "array":
        if not schema["minItems"] <= len(value) <= schema["maxItems"]:
            yield f"{location}: expected {schema['minItems']}..{schema['maxItems']} items"
            return
        for index, child in enumerate(value):
            if child in value[:index]:
                yield f"{location}[{index}]: duplicate evidence"
            yield from _schema_errors(child, schema["items"], f"{location}[{index}]")
    elif expected == "string":
        if not schema["minLength"] <= len(value) <= schema["maxLength"] or not re.search(schema["pattern"], value):
            yield f"{location}: expected nonblank string of at most {schema['maxLength']} characters"


def capability_usage(capabilities, sources):
    """Validate metadata and match anchors in a registered Rust source mapping.

    ``sources`` is ``{repository_relative_path: source_text}``, obtained by the
    caller from Cargo targets/ordinary production mod declarations. This pure
    helper never opens arbitrary metadata paths or infers absent capabilities.

    >>> capability_usage({'schema_version': True}, {})['passed']
    False
    >>> metadata = {'schema_version': 1, 'pause': {'simulation': {'evidence': [
    ...     {'path': 'examples/demo/src/lib.rs', 'snippet': 'session.toggle_pause()'}
    ... ]}}}
    >>> sources = {'examples/demo/src/lib.rs': 'fn menu() { session.toggle_pause(); }'}
    >>> capability_usage(metadata, sources)['claims'][0]['status']
    'source_matched'
    """
    report = {"schema_version": 1, "basis": "static-source", "runtime_verified": False,
              "passed": False, "claims": [], "errors": []}
    report["errors"] = [{"code": "capability_schema", "message": message}
                        for message in _schema_errors(capabilities, capabilities_schema())]
    if report["errors"]:
        return report
    tokenized = {}
    for group in CAPABILITY_FEATURES:
        for feature, claim in capabilities.get(group, {}).items():
            result = {"feature": f"{group}.{feature}", "status": "source_matched", "evidence": []}
            if "note" in claim:
                result["note"] = claim["note"]
            for anchor in claim["evidence"]:
                path, snippet = anchor["path"], anchor["snippet"]
                tokens = rust_tokens(snippet)
                problem = None
                if (PurePosixPath(path).is_absolute() or "\\" in path
                        or any(part in ("", ".", "..") for part in path.split("/"))
                        or not path.endswith(".rs")):
                    problem = "expected a canonical repository-relative Rust source path"
                elif path not in sources:
                    problem = "source is not a registered production module of this example"
                elif len(tokens) < 3 or "<literal>" in tokens or not any(re.fullmatch(r"[A-Za-z_]\w*", t) for t in tokens):
                    problem = "snippet needs at least three Rust tokens, an identifier and no literals"
                else:
                    if path not in tokenized:
                        tokenized[path] = production_tokens(sources[path])
                    haystack = tokenized[path]
                    if not any(haystack[i:i + len(tokens)] == tokens
                               for i in range(len(haystack) - len(tokens) + 1)):
                        problem = "snippet not found in production source tokens"
                result["evidence"].append({**anchor, "matched": problem is None})
                if problem:
                    result["status"] = "unverified"
                    report["errors"].append({"code": "capability_evidence", "path": path,
                                             "message": f"{result['feature']}: {problem}"})
            report["claims"].append(result)
    report["passed"] = not report["errors"]
    return report


def rust_tokens(source):
    """Tokenize Rust evidence without interpreting comments or string contents."""
    tokens = []
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            index = len(source) if end < 0 else end + 1
        elif source.startswith("/*", index):
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
        elif match := RAW_STRING.match(source, index):
            end = source.find('"' + match[1], match.end())
            index = len(source) if end < 0 else end + 1 + len(match[1])
            tokens.append("<literal>")
        elif match := QUOTED.match(source, index):
            index = match.end()
            tokens.append("<literal>")
        elif match := TOKEN.match(source, index):
            tokens.append(match[0])
            index = match.end()
        else:
            index += 1
    return tokens


def _closing(tokens, start, opening="{", closing="}"):
    depth = 0
    for index in range(start, len(tokens)):
        depth += tokens[index] == opening
        depth -= tokens[index] == closing
        if depth == 0:
            return index + 1
    return len(tokens)


def _cfg_without_test(tokens):
    """Evaluate cfg with test=false and all platform/feature predicates unknown."""
    if tokens == ["test"]:
        return False
    if len(tokens) < 3 or tokens[1] != "(" or tokens[-1] != ")":
        return None
    arguments = []
    start, depth = 2, 0
    for index in range(2, len(tokens) - 1):
        if tokens[index] == "," and depth == 0:
            arguments.append(tokens[start:index])
            start = index + 1
        depth += tokens[index] == "("
        depth -= tokens[index] == ")"
    if start < len(tokens) - 1:
        arguments.append(tokens[start:-1])
    values = [_cfg_without_test(arg) for arg in arguments]
    if tokens[0] == "not" and len(values) == 1:
        return None if values[0] is None else not values[0]
    if tokens[0] == "all":
        return False if False in values else None if None in values else True
    if tokens[0] == "any":
        return True if True in values else None if None in values else False
    return None


def production_tokens(source):
    """Exclude tests, cfg known false outside tests and unexpanded macro bodies.

    Unknown feature/target predicates stay eligible; this is not cfg resolution.
    Inner cfg attributes apply only to their enclosing file/inline module.
    """
    return _production_tokens(rust_tokens(source))


def _production_tokens(tokens):
    output = []
    index = 0
    while index < len(tokens):
        inner = tokens[index:index + 3] == ["#", "!", "["]
        if inner or tokens[index:index + 2] == ["#", "["]:
            opening = index + (2 if inner else 1)
            end = _closing(tokens, opening, "[", "]")
            attribute = tokens[opening + 1:end - 1]
            excluded = attribute == ["test"] or (
                attribute[:2] == ["cfg", "("] and attribute[-1:] == [")"]
                and _cfg_without_test(attribute[2:-1]) is False)
            if excluded:
                if inner:
                    return []
                index = end
                # Consume additional attributes before finding the item's body.
                while tokens[index:index + 2] == ["#", "["]:
                    index = _closing(tokens, index + 1, "[", "]")
                while index < len(tokens) and tokens[index] not in ("{", ";"):
                    index += 1
                index = _closing(tokens, index) if tokens[index:index + 1] == ["{"] else index + 1
                continue
        if tokens[index:index + 2] == ["macro_rules", "!"]:
            index += 3  # macro_rules! name { ... } is not an expanded item.
            if index < len(tokens) and tokens[index] in ("{", "[", "("):
                delimiter = tokens[index]
                index = _closing(tokens, index, delimiter, {"{": "}", "[": "]", "(": ")"}[delimiter])
            continue
        if tokens[index] == "{":
            end = _closing(tokens, index)
            output.extend(["{", *_production_tokens(tokens[index + 1:end - 1]), "}"])
            index = end
            continue
        output.append(tokens[index])
        index += 1
    return output


def _use_paths(tokens, prefix=()):
    """Expand Rust use trees into (qualified path, local alias) pairs."""
    index = 0
    while index < len(tokens):
        path = list(prefix)
        while index < len(tokens) and tokens[index] not in ("{", ",", "as", "}"):
            if tokens[index] != "::":
                path.append(tokens[index])
            index += 1
        if index < len(tokens) and tokens[index] == "{":
            end = _closing(tokens, index)
            yield from _use_paths(tokens[index + 1:end - 1], tuple(path))
            index = end
        elif path:
            alias = path[-1]
            if alias == "self":
                path.pop()
                alias = path[-1] if path else "self"
            if tokens[index:index + 1] == ["as"] and index + 1 < len(tokens):
                alias = tokens[index + 1]
                index += 2
            yield tuple(path), alias
        index += 1


def session_usage(sources):
    """Return migration evidence for a {repository_path: Rust_source} mapping.

    >>> session_usage({'main.rs': '// GameSession::new(60., 4, true)'})['status']
    'absent'
    >>> session_usage({'main.rs': '''use ferravex_core::session::GameSession;
    ... fn run() { let mut clock = GameSession::try_new(60., 4, true).unwrap();
    ... clock.advance(0.1); }'''})['status']
    'active'
    >>> session_usage({'main.rs': '''use ferravex_core::session::{GameSession, SessionConfig};
    ... fn run() { let config = SessionConfig::try_new(60., 4, true).unwrap();
    ... let mut clock = GameSession::from_config(config); clock.toggle_pause(); }'''})['status']
    'active'

    'active' means session construction and recognized receiver calls appear. It
    deliberately does not require a particular variable name, formatting,
    pause key, terminal state, or ownership of a multiplayer simulation clock.
    Recognized constructors are new, try_new and from_config on a resolved
    GameSession path, never SessionConfig construction alone. This static scan
    does not check constructor arguments, Result handling or runtime success.
    """
    evidence = []
    for path, source in sorted(sources.items()):
        tokens = production_tokens(source)
        imports = {}
        for index, token in enumerate(tokens):
            if token == "use":
                end = next((i for i in range(index, len(tokens)) if tokens[i] == ";"), len(tokens))
                for qualified, alias in _use_paths(tokens[index + 1:end]):
                    imports[alias] = qualified
        for qualified in list(imports.values()):
            if qualified[-1:] == ("*",) and qualified[:-1] + ("GameSession",) in SESSION_TYPES:
                imports["GameSession"] = qualified[:-1] + ("GameSession",)

        def resolve(path):
            seen = set()
            while path and path[0] in imports and path[0] not in seen:
                seen.add(path[0])
                path = imports[path[0]] + path[1:]
            return path

        constructors = 0
        references = 0
        receivers = set()
        for index, token in enumerate(tokens):
            if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", token):
                continue
            if index and tokens[index - 1] == "::":
                continue
            parts = [token]
            end = index + 1
            while tokens[end:end + 1] == ["::"] and end + 1 < len(tokens):
                parts.append(tokens[end + 1])
                end += 2
            qualified = resolve(tuple(parts))
            if qualified in SESSION_TYPES:
                references += 1
                # Field, parameter or annotated local: session: [&mut] Type.
                before = index - 1
                while before >= 0 and tokens[before] in ("&", "mut"):
                    before -= 1
                if before >= 1 and tokens[before] == ":":
                    receivers.add(tokens[before - 1])
            if (qualified[:-1] in SESSION_TYPES and qualified[-1] in SESSION_CONSTRUCTORS
                    and tokens[end:end + 1] == ["("]):
                constructors += 1
                # Inferred local/field/assignment: session =/: Type::constructor(...).
                # A trailing ?, unwrap() or expect(...) leaves this binding intact.
                if index >= 2 and tokens[index - 1] in ("=", ":"):
                    receivers.add(tokens[index - 2])
        methods = sorted({tokens[i + 2] for i in range(len(tokens) - 3)
                          if tokens[i] in receivers and tokens[i + 1] == "."
                          and tokens[i + 2] in SESSION_METHODS and tokens[i + 3] == "("})
        imported = any(resolve((alias,)) in SESSION_TYPES for alias in imports)
        if references or constructors or imported:
            evidence.append({"path": path, "constructors": constructors,
                             "receivers": sorted(receivers), "methods": methods})
    constructors = sum(item["constructors"] for item in evidence)
    methods = sorted({method for item in evidence for method in item["methods"]})
    status = "active" if constructors and methods else "constructed" if constructors else "referenced" if evidence else "absent"
    return {"status": status, "timing": "session" if "advance" in methods else "external_or_unobserved",
            "constructors": constructors, "methods": methods, "evidence": evidence}


class HostParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.games = []

    def handle_starttag(self, tag, attrs):
        for key, value in attrs:
            if key == "data-game":
                self.games.append(value)


def _file(root, relative, within=None):
    if not isinstance(relative, str) or not relative.strip() or "\\" in relative:
        raise ValueError("expected a nonempty repository-relative POSIX path")
    path = PurePosixPath(relative)
    if path.is_absolute() or any(part in (".", "..") for part in relative.split("/")):
        raise ValueError("path must not be absolute or contain traversal")
    resolved = (root / relative).resolve()
    if not resolved.is_relative_to(root.resolve()) or not resolved.is_relative_to((within or root).resolve()):
        raise ValueError("path escapes its source directory (including symlinks)")
    if not resolved.is_file():
        raise ValueError("file does not exist")
    return resolved


def _manifest(path):
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def _module_sources(root, entries, directory):
    """Follow ordinary mod declarations from Cargo targets, excluding test items.

    Unregistered src files, src/bin utilities and #[cfg(test)] modules cannot
    establish a demo migration. Custom Cargo entry paths are honored. Generated
    modules, include! and #[path] resolution require compiler/manual validation.
    """
    sources = {}
    visited = set()

    def modules(tokens, base):
        index = 0
        while index + 2 < len(tokens):
            if tokens[index:index + 2] == ["#", "["]:
                end = _closing(tokens, index + 1, "[", "]")
                if any(tokens[i:i + 2] == ["path", "="] for i in range(index + 2, end - 2)):
                    raise ValueError("custom #[path] modules require manual validation; refusing to guess a default source")
            if tokens[index] == "mod" and re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", tokens[index + 1]):
                name = tokens[index + 1]
                if tokens[index + 2] == ";":
                    candidates = [base / (name + ".rs"), base / name / "mod.rs"]
                    present = [path for path in candidates if path.is_file()]
                    if len(present) != 1:
                        raise ValueError(f"module {name} in {base.relative_to(root)} has {len(present)} source files; expected exactly one (custom #[path]/generated modules need manual validation)")
                    visit(present[0])
                    index += 3
                    continue
                if tokens[index + 2] == "{":
                    end = _closing(tokens, index + 2)
                    modules(tokens[index + 3:end - 1], base / name)
                    index = end
                    continue
            index += 1

    def visit(path):
        relative = str(path.relative_to(root))
        checked = _file(root, relative, directory)
        if checked in visited:
            return
        visited.add(checked)
        source = checked.read_text(encoding="utf-8")
        sources[relative] = source
        base = path.parent if path.name in ("lib.rs", "main.rs", "mod.rs") or path in entries else path.with_suffix("")
        modules(production_tokens(source), base)

    for entry in entries:
        visit(entry)
    return sources


def audit_catalog(root=ROOT, migration=False, require_session=False):
    """Return JSON-serializable diagnostics; never build or modify the checkout."""
    root = Path(root).resolve()
    report = {"schema_version": 1, "passed": False, "errors": [], "results": []}

    def error(target, code, message, path=None):
        target.append({"code": code, "message": str(message), **({"path": str(path)} if path else {})})

    try:
        catalog = json.loads((root / "examples/catalog.json").read_text(encoding="utf-8"),
                             object_pairs_hook=_unique_object)
        if not isinstance(catalog, list) or not catalog:
            raise ValueError("catalog must be a nonempty array")
        workspace = _manifest(root / "Cargo.toml")["workspace"]
        if not isinstance(workspace, dict) or not isinstance(workspace.get("members"), list):
            raise ValueError("workspace.members must be an array")
        for field in ("members", "exclude"):
            if not isinstance(workspace.get(field, []), list) or any(not isinstance(pattern, str) or not pattern or Path(pattern).is_absolute() for pattern in workspace.get(field, [])):
                raise ValueError(f"workspace.{field} must contain relative path patterns")
        members = {path.resolve() for pattern in workspace["members"] for path in root.glob(pattern) if path.is_dir()}
        excluded = {path.resolve() for pattern in workspace.get("exclude", []) for path in root.glob(pattern)}
        members -= excluded
    except (OSError, ValueError, KeyError, TypeError) as exc:
        error(report["errors"], "catalog_input", exc)
        catalog = []
        members = set()
    seen = {"id": set(), "host": set(), "image": set()}
    for number, game in enumerate(catalog):
        row = {"game": game.get("id") if isinstance(game, dict) else None, "passed": False, "errors": []}
        report["results"].append(row)
        errors = row["errors"]
        if not isinstance(game, dict):
            error(errors, "metadata", f"entry {number} must be an object")
            continue
        for field in ("id", "title", "dimension", "kind", "host", "genre", "description", "controls", "image", "source"):
            if not isinstance(game.get(field), str) or not game[field].strip():
                error(errors, "metadata", f"{field} must be a nonempty string")
        for field, values in (("dimension", {"2D", "3D"}), ("kind", {"game", "prototype", "lab"})):
            if not isinstance(game.get(field), str) or game[field] not in values:
                error(errors, "metadata", f"invalid {field}")
        for field in seen:
            value = game.get(field)
            if isinstance(value, str):
                if value in seen[field]:
                    error(errors, "duplicate", f"duplicate {field}: {value}")
                seen[field].add(value)
        game_id = game.get("id")
        if not isinstance(game_id, str) or not IDENTIFIER.fullmatch(game_id):
            error(errors, "metadata", "id must be a lowercase hyphenated package name")
            continue
        directory = root / "examples" / game_id
        if directory.resolve() not in members:
            error(errors, "workspace", "example is not a workspace member", directory.relative_to(root))
        paths = {"source": (game.get("source"), directory),
                 "readme": (f"examples/{game_id}/README.md", directory)}
        if "editable_scene" in game:
            paths["editable_scene"] = (game["editable_scene"], directory)
        host = game.get("host")
        if not isinstance(host, str) or not IDENTIFIER.fullmatch(host):
            error(errors, "host", "host must be a single directory name")
        else:
            paths["host"] = (f"web/{host}/index.html", root / "web")
        image = game.get("image")
        if not isinstance(image, str) or PurePosixPath(image).name != image or not image.endswith(".png"):
            error(errors, "image", "image must be a PNG filename")
        else:
            paths["image"] = (f"docs/screenshots/{image}", root / "docs/screenshots")
        try:
            manifest_path = _file(root, f"examples/{game_id}/Cargo.toml", root / "examples")
            manifest = _manifest(manifest_path)
            if not isinstance(manifest.get("package"), dict) or not isinstance(manifest.get("lib", {}), dict):
                raise ValueError("package and lib must be TOML tables")
            if not isinstance(manifest.get("bin", []), list) or any(not isinstance(item, dict) for item in manifest.get("bin", [])):
                raise ValueError("bin must be an array of target tables")
            if manifest.get("package", {}).get("name") != game_id:
                error(errors, "package", "package name does not match catalog id")
            lib = manifest.get("lib", {})
            if not isinstance(lib.get("crate-type", []), list) or "cdylib" not in lib.get("crate-type", []):
                error(errors, "wasm_target", "library must produce a cdylib for the browser host")
            if lib.get("name", game_id.replace("-", "_")) != game_id.replace("-", "_"):
                error(errors, "wasm_target", "library name does not match the staged Wasm filename")
            paths["lib"] = (f"examples/{game_id}/{lib.get('path', 'src/lib.rs')}", directory)
            binary = next((target for target in manifest.get("bin", []) if target.get("name") == game_id), None)
            if binary is None and manifest.get("package", {}).get("autobins", True) is False:
                error(errors, "native_target", "no native binary named after the catalog id")
            paths["bin"] = (f"examples/{game_id}/{(binary or {}).get('path', 'src/main.rs')}", directory)
        except (OSError, ValueError, TypeError) as exc:
            error(errors, "manifest", exc, f"examples/{game_id}/Cargo.toml")
        for field, (relative, within) in paths.items():
            try:
                path = _file(root, relative, within)
                if field == "host":
                    parser = HostParser()
                    parser.feed(path.read_text(encoding="utf-8"))
                    if any(value != game_id for value in parser.games):
                        raise ValueError("host data-game does not match catalog id")
            except (OSError, ValueError) as exc:
                error(errors, "path", f"{field}: {exc}", relative)
        if migration or require_session or "capabilities" in game:
            sources = {}
            try:
                entries = [root / paths[field][0] for field in ("lib", "bin") if field in paths]
                sources = _module_sources(root, entries, directory)
            except (OSError, ValueError) as exc:
                error(errors, "session_source" if migration or require_session else "capability_source", exc)
            else:
                if migration or require_session:
                    row["migration"] = session_usage(sources)
                    if require_session and row["migration"]["status"] != "active":
                        error(errors, "session_migration", "no constructed session with observed lifecycle/timing calls; inspect migration evidence")
            if "capabilities" in game:
                row["capabilities"] = capability_usage(game["capabilities"], sources)
                errors.extend(row["capabilities"]["errors"])
        row["passed"] = not errors
    # Reconcile in both directions: dropping a demo from JSON must not hide it.
    discovered = {path.parent.name for path in (root / "examples").glob("*/Cargo.toml")}
    workspace_examples = {path.name for path in members if path.parent == root / "examples"}
    for missing in sorted((discovered | workspace_examples) - seen["id"]):
        error(report["errors"], "missing_game", f"example missing from catalog: {missing}")
    report["summary"] = {"games": len(catalog), "passed": sum(row["passed"] for row in report["results"]),
                         "failed": sum(not row["passed"] for row in report["results"]),
                         "errors": len(report["errors"]) + sum(len(row["errors"]) for row in report["results"])}
    if migration or require_session:
        report["summary"]["migration"] = dict(sorted(Counter(row["migration"]["status"] for row in report["results"] if "migration" in row).items()))
        report["migration_note"] = "Static source evidence from Cargo entry points and ordinary mod declarations, including target-specific code. No macro/include!/custom #[path] expansion or full name resolution; does not prove compilation, clock authority or runtime pause/retry behavior."
    capability_rows = [row["capabilities"] for row in report["results"] if "capabilities" in row]
    if capability_rows:
        report["capability_note"] = CAPABILITY_NOTE
        report["summary"]["capabilities"] = {
            "declared_games": len(capability_rows),
            "source_matched": sum(claim["status"] == "source_matched"
                                  for row in capability_rows for claim in row["claims"]),
            "unverified": sum(claim["status"] != "source_matched"
                              for row in capability_rows for claim in row["claims"]),
            "runtime_verified": False,
        }
    report["passed"] = not report["errors"] and bool(catalog) and all(row["passed"] for row in report["results"])
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="write only the machine-readable report to stdout")
    parser.add_argument("--output", type=Path, help="also save the JSON report")
    parser.add_argument("--migration", action="store_true", help="report GameSession evidence without requiring migration")
    parser.add_argument("--require-session", action="store_true", help="fail when a demo lacks constructed session usage evidence")
    parser.add_argument("--capabilities-schema", action="store_true", help="print the optional capabilities v1 JSON Schema without auditing")
    args = parser.parse_args(argv)
    if args.capabilities_schema and (args.migration or args.require_session):
        parser.error("--capabilities-schema cannot be combined with migration checks")
    report = capabilities_schema() if args.capabilities_schema else audit_catalog(
        root=ROOT, migration=args.migration, require_session=args.require_session)
    encoded = json.dumps(report, indent=2) + "\n"
    if args.output:
        try:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(encoded, encoding="utf-8")
        except OSError as exc:
            print(f"Cannot write catalog report: {exc}", file=sys.stderr)
            return 1
    if args.json or args.capabilities_schema:
        print(encoded, end="")
    else:
        for row in report["results"]:
            migration = f"; session={row['migration']['status']}" if "migration" in row else ""
            capabilities = f"; static-capabilities={len(row['capabilities']['claims'])}" if "capabilities" in row else ""
            print(f"{'PASS' if row['passed'] else 'FAIL'} {row['game']}{migration}{capabilities}")
            for error in row["errors"]:
                print(f"  {error['code']}: {error['message']} ({error.get('path', row['game'])})")
        for error in report["errors"]:
            print(f"FAIL {error['code']}: {error['message']}")
        print(json.dumps(report["summary"], sort_keys=True))
        if "migration_note" in report:
            print(report["migration_note"])
        if "capability_note" in report:
            print(report["capability_note"])
    return 0 if args.capabilities_schema or report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
