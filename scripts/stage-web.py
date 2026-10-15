#!/usr/bin/env python3
"""Stage built Wasm and the pinned Macroquad loader with a strict-mode fix."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = tuple(game["id"] for game in json.loads((ROOT / "examples" / "catalog.json").read_text()))
MACROQUAD_VERSION = "0.4.16"


def patch_macroquad_loader(source):
    """Fix 0.4.16's undeclared quad_net registration variable in strict mode.

    Keep the plugin and its registration intact. Reject changed upstream input
    instead of silently applying a broader replacement or hiding script errors.
    """
    original = b"function i(){}register_plugin=function"
    # A separate declaration preserves the following comma expression, which
    # would be invalid if it became part of a `var x = ... , ...` declaration.
    replacement = b"function i(){}var register_plugin;register_plugin=function"
    occurrences = source.count(original)
    if occurrences != 1:
        raise ValueError(
            f"macroquad {MACROQUAD_VERSION} loader patch expected one exact match; found {occurrences}"
        )
    return source.replace(original, replacement, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=("release", "small"), default="release", nargs="?")
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1",
         "--filter-platform", "wasm32-unknown-unknown"], cwd=ROOT, text=True,
    ))
    packages = [p for p in metadata["packages"] if p["name"] == "macroquad"]
    if len(packages) != 1 or packages[0]["version"] != MACROQUAD_VERSION:
        parser.error(f"expected exactly one resolved macroquad {MACROQUAD_VERSION}; update the loader pin deliberately")
    package = packages[0]
    if not str(package.get("source", "")).startswith("registry+"):
        parser.error("expected macroquad from the Cargo registry")
    macroquad = Path(package["manifest_path"]).parent
    artifacts = Path(metadata["target_directory"]) / "wasm32-unknown-unknown" / args.profile
    files = {f"{name}.wasm": artifacts / f"{name}.wasm" for name in EXAMPLES}
    files.update({"index.html": ROOT / "web" / "index.html",
                  "gallery.html": ROOT / "web" / "gallery.html",
                  "games.json": ROOT / "examples" / "catalog.json",
                  "mq_js_bundle.js": macroquad / "js" / "mq_js_bundle.js"})
    files.update({f"licenses/macroquad-{name}": macroquad / name
                  for name in ("LICENSE-MIT", "LICENSE-APACHE")})
    files.update({f"screenshots/{path.name}": path for path in (ROOT / "docs" / "screenshots" / "games").glob("*.png")})
    missing = [str(path) for path in files.values() if not path.is_file()]
    if missing:
        parser.error("missing build inputs:\n" + "\n".join(missing))
    loader_source = files["mq_js_bundle.js"].read_bytes()
    try:
        patched_loader = patch_macroquad_loader(loader_source)
    except ValueError as error:
        parser.error(str(error))
    output = ROOT / "dist" / "web"
    for name, source in files.items():
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        if name == "mq_js_bundle.js":
            destination.write_bytes(patched_loader)
        else:
            shutil.copyfile(source, destination)
    manifest = {
        "profile": args.profile,
        "macroquad": MACROQUAD_VERSION,
        "loader_patch": {
            "name": "quad_net-strict-mode-register-plugin-declaration",
            "upstream_sha256": hashlib.sha256(loader_source).hexdigest(),
        },
        "files": {name: {"bytes": (output / name).stat().st_size,
                         "sha256": hashlib.sha256((output / name).read_bytes()).hexdigest()}
                  for name in files},
    }
    (output / "build-info.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Staged {', '.join(EXAMPLES)} ({args.profile}) in {output}")
    print("Serve with: python3 scripts/serve-web.py")


if __name__ == "__main__":
    main()
