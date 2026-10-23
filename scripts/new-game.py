#!/usr/bin/env python3
"""Create a standalone Ferravex game without changing the engine workspace.

Usage: python3 scripts/new-game.py moon-garden ../moon-garden --template 3d
       --tick-rate 120 --window-title 'Moon Garden'
Requires Python 3.11+; generation uses only the standard library, without Cargo.
"""

import argparse
import html
import json
import math
from pathlib import Path
import re
import shlex
import shutil
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
RESERVED = set("""
    as async await become box break const continue crate do dyn else enum extern
    false final fn for gen if impl in let loop macro match mod move mut override
    priv pub ref return self static struct super trait true try type typeof unsafe
    unsized use virtual where while yield abstract union
    build deps examples incremental test con prn aux nul
""".split()) | {f"{prefix}{n}" for prefix in ("com", "lpt") for n in range(10)}
TOKEN = re.compile(r"@@([A-Z_]+)@@")


def validate_tick_rate(tick_rate: float) -> float:
    """Return finite Hz in the engine's 1..=1000 range, or raise ValueError.

    Integers and floats are accepted; booleans and implicit string conversions
    are rejected. CLI conversion happens in argparse before this validation.
    """
    if (
        isinstance(tick_rate, bool)
        or not isinstance(tick_rate, (int, float))
        or not 1 <= tick_rate <= 1000
        or not math.isfinite(tick_rate)
    ):
        raise ValueError("tick rate must be a finite number from 1 to 1000 Hz")
    return float(tick_rate)


def validate_window_title(window_title: str) -> str:
    """Return a nonblank title of 1–120 printable Unicode characters.

    Reject control characters (including NUL/newlines) and unpaired surrogates
    before rendering Rust, HTML or JSON. Preserve the supplied text verbatim.
    """
    if (
        not isinstance(window_title, str)
        or not 1 <= len(window_title) <= 120
        or not window_title.strip()
        or not window_title.isprintable()
    ):
        raise ValueError("window title must contain 1–120 printable characters and not be blank")
    return window_title


def validate_name(name: str) -> str:
    """Return a portable Cargo name, or raise ValueError before writing files."""
    if not re.fullmatch(r"[a-z][a-z0-9]*(?:[-_][a-z0-9]+)*", name) or len(name) > 64:
        raise ValueError("name must be 1–64 lowercase ASCII letters/digits separated by '-' or '_', starting with a letter")
    if name in RESERVED or name.replace("-", "_") in RESERVED:
        raise ValueError(f"{name!r} is a reserved Rust, Cargo or platform name")
    if name == "ferravex" or name.startswith(("ferravex-", "ferravex_")):
        raise ValueError("choose a game name outside the reserved ferravex engine namespace")
    return name


def standalone_lock(engine_root: Path, name: str) -> str:
    """Seed the new project's lock from the checkout's first-party facade closure.

    The checkout lock is read only. Keeping just reachable packages lets the
    generated game use --offline --locked on its very first build.
    """
    lock = tomllib.loads((engine_root / "Cargo.lock").read_text(encoding="utf-8"))
    packages = {package["name"]: package for package in lock["package"]}
    selected = {}

    def visit(package_name):
        if package_name in selected:
            return
        package = packages.get(package_name)
        if package is None or "source" in package or "checksum" in package:
            raise ValueError(f"checkout lock must contain first-party path package {package_name!r}")
        selected[package_name] = package
        for dependency in package.get("dependencies", []):
            visit(dependency)

    visit("ferravex")
    if name in selected:
        raise ValueError(f"name {name!r} conflicts with an engine dependency")
    selected[name] = {"name": name, "version": "0.1.0", "dependencies": ["ferravex"]}
    lines = ["# Seeded from the Ferravex checkout; managed by Cargo after generation.", "version = 4", ""]
    for package_name, package in sorted(selected.items()):
        lines.extend(["[[package]]", f"name = {json.dumps(package_name)}", f"version = {json.dumps(package['version'])}"])
        if package.get("dependencies"):
            lines.append("dependencies = " + json.dumps(package["dependencies"]))
        lines.append("")
    return "\n".join(lines)


def generate(
    name: str,
    destination: Path,
    template: str = "2d",
    *,
    engine_root: Path = ROOT,
    tick_rate: float = 60.0,
    window_title: str | None = None,
) -> Path:
    """Create one new project and return its absolute path.

    The destination's parent must already exist. Existing directories, files and
    symlinks are always rejected, including empty directories and dangling links.
    Validation/rendering finishes before the destination is created; a write
    failure removes only the new directory created by this invocation.

    tick_rate accepts finite 1..=1000 Hz; window_title defaults to the package
    name and accepts 1–120 printable, nonblank characters. ferravex.json records
    schema_version 1, the resolved options and the sorted generated file list.
    """
    validate_name(name)
    if template not in ("2d", "3d"):
        raise ValueError("template must be '2d' or '3d'")
    tick_rate = validate_tick_rate(tick_rate)
    window_title = validate_window_title(name if window_title is None else window_title)
    destination = Path(destination).expanduser()
    if destination.name in ("", ".", "..") or any(ord(c) < 32 for c in str(destination)):
        raise ValueError("destination must name a new directory and contain no control characters")
    if destination.exists() or destination.is_symlink():
        raise ValueError(f"refusing to overwrite existing destination: {destination}")
    if not destination.parent.is_dir():
        raise ValueError(f"destination parent does not exist: {destination.parent}; create it first")
    destination = destination.parent.resolve() / destination.name
    engine_root = engine_root.resolve(strict=True)
    if not (engine_root / "crates/ferravex/Cargo.toml").is_file():
        raise ValueError(f"not a Ferravex checkout: {engine_root}")
    tokens = {
        "NAME": name,
        "CRATE": name.replace("-", "_"),
        "DIMENSION": template.upper(),
        "FACADE_TOML": json.dumps((engine_root / "crates/ferravex").as_posix(), ensure_ascii=False),
        "ENGINE_JSON": json.dumps(engine_root.as_posix(), ensure_ascii=False),
        "TICK_RATE": repr(tick_rate),
        # Printable Unicode excludes JSON-only control escapes, so this quoted
        # literal also works in Rust. Never substitute raw titles into source.
        "WINDOW_TITLE_RUST": json.dumps(window_title, ensure_ascii=False),
        "WINDOW_TITLE_JSON": json.dumps(window_title, ensure_ascii=False),
        "WINDOW_TITLE_HTML": html.escape(window_title, quote=True),
    }
    source = engine_root / "templates" / f"game-{template}"
    paths = sorted(source.rglob("*.in"))
    generated_files = {path.relative_to(source).with_suffix("").as_posix() for path in paths}
    generated_files.add("Cargo.lock")
    tokens["GENERATED_FILES_JSON"] = json.dumps(sorted(generated_files), indent=2).replace("\n", "\n  ")
    files = {}
    for path in paths:
        text = path.read_text(encoding="utf-8")
        # One substitution pass: paths/titles containing @@...@@ are literal data.
        text = TOKEN.sub(lambda match: tokens[match[1]], text)
        files[path.relative_to(source).with_suffix("")] = text
    if not {Path("Cargo.toml"), Path("src/lib.rs"), Path("ferravex.json")} <= files.keys():
        raise ValueError(f"incomplete template: {source}")
    files[Path("Cargo.lock")] = standalone_lock(engine_root, name)
    destination.mkdir()  # Exclusive creation also catches a racing generator.
    try:
        for relative, text in files.items():
            output = destination / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            with output.open("x", encoding="utf-8", newline="\n") as stream:
                stream.write(text)
    except BaseException:
        shutil.rmtree(destination)
        raise
    return destination


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name", help="lowercase Cargo package name, e.g. moon-garden")
    parser.add_argument("destination", nargs="?", help="new directory (default: ./NAME); parent must exist")
    parser.add_argument("--template", choices=("2d", "3d"), default="2d")
    parser.add_argument("--tick-rate", type=float, default=60.0, metavar="HZ",
                        help="finite simulation tick rate from 1 to 1000 Hz (default: 60)")
    parser.add_argument("--window-title", metavar="TITLE",
                        help="1–120 printable, nonblank characters (default: package name)")
    args = parser.parse_args(argv)
    try:
        if args.destination == "":
            raise ValueError("destination cannot be empty")
        destination = generate(
            args.name, Path(args.destination or args.name), args.template,
            tick_rate=args.tick_rate, window_title=args.window_title,
        )
    except (OSError, ValueError, KeyError) as error:
        print(f"new-game: {error}", file=sys.stderr)
        return 1
    print(f"Created {args.template.upper()} game {args.name} at {destination}")
    print(f"  cd {shlex.quote(str(destination))}")
    print("  cargo run --offline --locked --jobs 2")
    print("Browser build: python3 -B scripts/build-web.py")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
