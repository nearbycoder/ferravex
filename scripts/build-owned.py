#!/usr/bin/env python3
"""Build/stage first-party browser games and a reproducible build-info.json.

Manifest schema 1 retains ``profile``, ``external_packages`` and ``files``.
File keys are sorted POSIX paths; hashes describe staged bytes, excluding the
manifest itself. ``source`` fingerprints current build sources, including local
edits and untracked files, independently of Git availability. Its explicit scope
excludes generated/cache directories outside ``web`` (host assets are unfiltered).
No timestamps or absolute paths are stored.
This records provenance; it does not promise identical Rust output across tools,
platforms or compiler environment overrides.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
TARGET = "wasm32-unknown-unknown"
PROFILES = ("release", "small", "dev")
MANIFEST = "build-info.json"
SOURCE_SCOPE = (
    "Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
    ".cargo", "crates", "examples", "web", "scripts",
)
SOURCE_EXCLUDES = (".git", "__pycache__", "dist", "target")
SHARED_ASSETS = (
    "runtime.js", "lifecycle.js", "visuals.js", "status.js", "details.js", "player.js",
    "loader.js", "fullscreen.js", "audio.js", "audio-worklet.js", "audio-ring.js",
    "input.js", "storage.js", "input-overlay.js", "retained.js", "effects.js",
    "material-tools.js", "content-tools.js", "replay-tools.js",
    "multiplayer-tools.js", "index.html", "catalog.js",
)
SHADERS = {
    "retained-shader.js": (
        ("retained.rs", "VERTEX_BODY", "vertexBody"),
        ("material.rs", "FRAGMENT_BODY", "fragmentBody"),
        ("material.rs", "DEFAULT_VERTEX_HOOK", "defaultVertexHook"),
        ("material.rs", "DEFAULT_FRAGMENT_HOOK", "defaultFragmentHook"),
    ),
    "effects-shader.js": (
        ("effects.rs", "SPRITE_VERTEX_BODY", "spriteVertexBody"),
        ("effects.rs", "SPRITE_FRAGMENT_BODY", "spriteFragmentBody"),
    ),
}
CONTENT_EXPORTS = frozenset("ferravex_content_" + name for name in (
    "document", "source", "source_len", "generation", "status", "revision",
    "message", "message_len", "edit_buffer", "submit",
))


def _json_bytes(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def _regular_file(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f"expected a regular file (no symlinks): {path}")


def _tree_files(directory, *, source=False):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError(f"expected a directory (no symlinks): {directory}")

    def fail(error):
        raise error

    for base, directories, names in os.walk(directory, onerror=fail):
        directories[:] = sorted(name for name in directories
                                if not source or name not in SOURCE_EXCLUDES)
        for name in directories:
            if (Path(base) / name).is_symlink():
                raise ValueError(f"source directory is a symlink: {Path(base) / name}")
        for name in sorted(names):
            if source and name.endswith((".pyc", ".pyo")):
                continue
            path = Path(base) / name
            _regular_file(path)
            yield path


def _file_record(path):
    _regular_file(path)
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
            size += len(chunk)
    return {"bytes": size, "sha256": digest.hexdigest()}


def artifact_manifest(output: Path) -> dict:
    """Hash the complete staged tree, excluding only its root build-info.json.

    Example: ``artifact_manifest(Path("dist/web"))["runtime.js"]["sha256"]``.
    Symlinks and non-regular files fail instead of silently escaping the tree.
    """
    return dict(sorted(
        (path.relative_to(output).as_posix(), _file_record(path))
        for path in _tree_files(output) if path != output / MANIFEST
    ))


def _git(root, *arguments):
    try:
        result = subprocess.run(
            ["git", *arguments], cwd=root, capture_output=True, text=True,
            encoding="utf-8", errors="replace", check=False,
        )
    except FileNotFoundError:
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def source_provenance(root: Path) -> dict:
    """Fingerprint SOURCE_SCOPE's working bytes, with optional Git HEAD/dirty.

    ``source_provenance(root)["sha256"]`` hashes canonical JSON of the relative
    source file inventory, so dirty/untracked sources and deletions affect it.
    Missing Git/a source archive yields null Git fields, never a false clean HEAD.
    """
    files = {}
    for name in SOURCE_SCOPE:
        path = root / name
        if not path.exists() and not path.is_symlink():
            continue
        paths = _tree_files(path, source=name != "web") if path.is_dir() else (path,)
        for item in paths:
            files[item.relative_to(root).as_posix()] = _file_record(item)
    files = dict(sorted(files.items()))
    top = _git(root, "rev-parse", "--show-toplevel")
    is_checkout = top is not None and Path(top).resolve() == root.resolve()
    revision = _git(root, "rev-parse", "--verify", "HEAD") if is_checkout else None
    status = _git(root, "status", "--porcelain", "--untracked-files=normal") if is_checkout else None
    return {
        "revision": revision,
        "dirty": bool(status) if status is not None else None,
        "scope": list(SOURCE_SCOPE),
        "unfiltered_scopes": ["web"],
        "excluded_directories": list(SOURCE_EXCLUDES),
        "excluded_suffixes": [".pyc", ".pyo"],
        "sha256": hashlib.sha256(_json_bytes(files)).hexdigest(),
        "files": files,
    }


def load_catalog(root: Path) -> list[dict]:
    """Read catalog build entries, rejecting unsafe/duplicate artifact paths."""
    games = json.loads((root / "examples/catalog.json").read_text(encoding="utf-8"))
    if not isinstance(games, list) or not games:
        raise ValueError("game catalog must be a nonempty array")
    seen_ids, seen_hosts, seen_modules = set(), set(), set()
    for game in games:
        if not isinstance(game, dict):
            raise ValueError("catalog game must be an object")
        for field in ("id", "host"):
            value = game.get(field)
            if not isinstance(value, str) or not re.fullmatch(r"[a-z][a-z0-9_-]*", value):
                raise ValueError(f"invalid catalog {field}: {value!r}")
        module = game["id"].replace("-", "_")
        if game["id"] in seen_ids or game["host"] in seen_hosts or module in seen_modules:
            raise ValueError(f"duplicate catalog package, host or module: {game['id']}")
        seen_ids.add(game["id"])
        seen_hosts.add(game["host"])
        seen_modules.add(module)
        _regular_file(root / "web" / game["host"] / "index.html")
        if "editable_scene" in game:
            scene = game["editable_scene"]
            if not isinstance(scene, str) or "\\" in scene:
                raise ValueError(f"invalid editable_scene for {game['id']}")
            path = PurePosixPath(scene)
            if (path.is_absolute() or ".." in path.parts or path.as_posix() != scene
                    or not path.is_relative_to("examples/" + game["id"])):
                raise ValueError(f"invalid editable_scene for {game['id']}: {scene!r}")
            _regular_file(root / scene)
    return sorted(games, key=lambda game: game["id"])


def _read_u32(stream, end):
    value = 0
    for shift in range(0, 35, 7):
        if stream.tell() >= end:
            raise ValueError("truncated Wasm integer")
        byte = stream.read(1)[0]
        value |= (byte & 127) << shift
        if value > 0xffffffff:
            break
        if not byte & 128:
            return value
    raise ValueError("invalid Wasm u32 integer")


def _content_exports(path):
    # Inspect only the bounded export section. This is an ABI guard, not a Wasm
    # validator or an execution test; Node's real-Wasm tests remain necessary.
    names = set()
    with path.open("rb") as stream:
        size = os.fstat(stream.fileno()).st_size
        if stream.read(8) != b"\0asm\x01\0\0\0":
            raise ValueError("expected a Wasm version 1 module")
        seen_exports = False
        while stream.tell() < size:
            section = stream.read(1)[0]
            length = _read_u32(stream, size)
            end = stream.tell() + length
            if end > size:
                raise ValueError("truncated Wasm section")
            if section != 7:
                stream.seek(end)
                continue
            if seen_exports:
                raise ValueError("duplicate Wasm export section")
            seen_exports = True
            count = _read_u32(stream, end)
            for _ in range(count):
                length = _read_u32(stream, end)
                if length > end - stream.tell() - 2 or length > 1024 * 1024:
                    raise ValueError("invalid Wasm export name length")
                name = stream.read(length).decode("utf-8")
                kind = stream.read(1)[0]
                _read_u32(stream, end)
                if name.startswith("ferravex_content_"):
                    if kind != 0 or name in names:
                        raise ValueError("invalid authoring function export")
                    names.add(name)
            if stream.tell() != end:
                raise ValueError("invalid Wasm export section length")
    return names


def _check_variant(path, game):
    try:
        names = _content_exports(path)
        if game.get("editable_scene"):
            missing = CONTENT_EXPORTS - names
            if missing:
                raise ValueError("missing authoring exports: " + ", ".join(sorted(missing)))
        elif names:
            raise ValueError("unexpected authoring exports in runtime game")
    except ValueError as error:
        raise ValueError(f"{game['id']} ({path.name}): {error}") from error


def _stage_hosts(root, staged, games):
    for game in games:
        host = root / "web" / game["host"]
        for path in _tree_files(host):
            relative = path.relative_to(host)
            if relative.as_posix() == game["id"].replace("-", "_") + ".wasm":
                raise ValueError(f"host asset collides with built Wasm: {path}")
            destination = staged / game["host"] / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)
    for name in SHARED_ASSETS:
        shutil.copy2(root / "web" / name, staged / name)
    shutil.copy2(root / "examples/catalog.json", staged / "games.json")
    for output, templates in SHADERS.items():
        parts = []
        for file, name, export in templates:
            source = (root / "crates/ferravex-render/src" / file).read_text(encoding="utf-8")
            match = re.search(r"pub const " + name + r':\s*&str\s*=\s*r#"(.*?)"#;', source, re.S)
            if not match:
                raise ValueError("Missing shared shader template: " + name)
            parts.append("export const " + export + " = " + json.dumps(match.group(1)) + ";")
        (staged / output).write_bytes(("\n".join(parts) + "\n").encode("utf-8"))


def _publish(staged, output, backup):
    if output.is_symlink() or (output.exists() and not output.is_dir()):
        raise ValueError(f"expected an output directory (no symlinks): {output}")
    had_output = output.exists()
    if had_output:
        if backup.exists() or backup.is_symlink():
            raise ValueError(f"output backup already exists: {backup}")
        output.rename(backup)
    try:
        staged.rename(output)
    except OSError as error:
        if had_output:
            try:
                backup.rename(output)
            except OSError as rollback:
                raise OSError(f"publication failed; previous output retained at {backup}: {rollback}") from error
        raise
    if had_output:
        shutil.rmtree(backup)


def build(profile: str = "release", *, root: Path = ROOT) -> dict:
    """Build isolated runtime/authoring groups, then replace root/dist/web.

    Returns the existing per-module size report. ``build("small", root=fixture)``
    uses that checkout only. Cargo is explicitly pinned to root/target, offline,
    locked and two jobs. Pre-publication failures preserve the staged web tree;
    a failed replacement rolls back, or reports its retained backup for recovery.
    """
    if profile not in PROFILES:
        raise ValueError(f"unsupported build profile: {profile!r}")
    root = root.resolve()
    source = source_provenance(root)
    games = load_catalog(root)
    subprocess.run([sys.executable, "-B", "scripts/audit-owned.py"], cwd=root, check=True)
    output = root / "dist/web"
    output.parent.mkdir(parents=True, exist_ok=True)
    folder = "debug" if profile == "dev" else profile
    report, variants, groups = {}, {}, []
    with tempfile.TemporaryDirectory(prefix=".web-build-", dir=output.parent) as temporary:
        staged = Path(temporary) / "staged"
        staged.mkdir()
        _stage_hosts(root, staged, games)
        # Separate invocations prevent Cargo feature unification from leaking
        # the editing ABI into runtime games. Capture each group immediately.
        for editing in (False, True):
            group = [game for game in games if bool(game.get("editable_scene")) == editing]
            if not group:
                continue
            variant = "authoring" if editing else "runtime"
            command = [
                "cargo", "build", "--offline", "--locked", "--lib", "--jobs", "2",
                "--target", TARGET, "--target-dir", str(root / "target"),
                "--profile", profile,
            ]
            for game in group:
                command.extend(["-p", game["id"]])
            subprocess.run(command, cwd=root, check=True)
            groups.append({"variant": variant, "packages": [game["id"] for game in group]})
            for game in group:
                module = game["id"].replace("-", "_")
                wasm = root / "target" / TARGET / folder / (module + ".wasm")
                _regular_file(wasm)
                destination = staged / game["host"] / wasm.name
                shutil.copy2(wasm, destination)
                _check_variant(destination, game)
                variants[game["id"]] = {
                    "host": game["host"], "variant": variant,
                    "wasm": destination.relative_to(staged).as_posix(),
                    "editable_scene": game.get("editable_scene"),
                }
        if source_provenance(root) != source:
            raise ValueError("build sources changed during staging; retry with stable sources")
        manifest = {
            "schema_version": 1, "profile": profile, "target": TARGET,
            "external_packages": 0, "source": source, "build_groups": groups,
            "games": variants, "files": artifact_manifest(staged),
        }
        (staged / MANIFEST).write_bytes(_json_bytes(manifest))
        for game in games:
            module = game["id"].replace("-", "_")
            prefix = game["host"] + "/"
            report[module] = {
                "profile": profile,
                "wasm_bytes": manifest["files"][variants[game["id"]]["wasm"]]["bytes"],
                "files": {path[len(prefix):]: record["bytes"]
                          for path, record in manifest["files"].items() if path.startswith(prefix)},
                "third_party_runtime_packages": 0,
            }
        # Keep a recovery backup outside TemporaryDirectory's cleanup in case
        # both publication and rollback fail (for example, a filesystem error).
        _publish(staged, output, Path(temporary + "-previous"))
    return report


def main(argv: list[str] | None = None) -> int:
    """CLI entry point; importing this module never builds or stages files."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=PROFILES, default="release")
    args = parser.parse_args(argv)
    try:
        report = build(args.profile)
    except subprocess.CalledProcessError as error:
        print(f"web build failed: {error}", file=sys.stderr)
        return error.returncode if error.returncode > 0 else 1
    except (OSError, ValueError) as error:
        print(f"web build failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
