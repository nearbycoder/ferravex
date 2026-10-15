#!/usr/bin/env python3
"""Report actual artifact bytes and deterministic gzip-9 transfer estimates."""

import argparse
import gzip
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
GAMES = json.loads((ROOT / "examples" / "catalog.json").read_text())
EXAMPLES = tuple(game['id'] for game in GAMES)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release", "small"), default="release")
    parser.add_argument("--target", help="native cross-compilation target, if built with cargo --target")
    parser.add_argument("--json", action="store_true", help="print machine-readable results")
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        cwd=ROOT, text=True,
    ))
    target = Path(metadata["target_directory"])
    native = (target / args.target if args.target else target) / args.profile
    wasm = target / "wasm32-unknown-unknown" / args.profile
    candidates = [(f"native/{name}", native / name) for name in (*EXAMPLES, "ferravex-server")]
    candidates += [(f"wasm/{name.replace('-', '_')}.wasm", wasm / f"{name.replace('-', '_')}.wasm") for name in EXAMPLES]
    for game in GAMES:
        candidates += [(f"web/{game['host']}/{path.name}", path) for path in (ROOT / 'dist/web' / game['host']).glob('*') if path.is_file() and path.suffix != '.wasm']
    candidates += [(f"web/{name}", ROOT / "dist/web" / name) for name in ("runtime.js", "input.js", "input-overlay.js", "retained.js", "retained-shader.js", "material-tools.js")]
    rows = []
    missing = []
    for label, path in candidates:
        if label.startswith("native/") and not path.is_file() and path.with_suffix(".exe").is_file():
            path = path.with_suffix(".exe")
        if not path.is_file():
            missing.append(label)
            continue
        data = path.read_bytes()
        rows.append({"artifact": label, "path": str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path), "bytes": len(data),
                     "gzip9_bytes": len(gzip.compress(data, compresslevel=9, mtime=0))})
    if not rows:
        parser.error(f"no built artifacts found for profile {args.profile}; build native examples or run scripts/build-web.sh")
    if args.json:
        print(json.dumps({"profile": args.profile, "artifacts": rows, "missing": missing}, indent=2))
    else:
        print(f"Profile: {args.profile}; exact bytes, gzip level 9 (mtime=0)")
        print(f"{'Artifact':30} {'File bytes':>14} {'Gzip bytes':>14}")
        for row in rows:
            print(f"{row['artifact']:30} {row['bytes']:14,d} {row['gzip9_bytes']:14,d}")
        if missing:
            print("Not built: " + ", ".join(missing))
        print("Gzip values are transfer estimates, not executable size or frame-time measurements.")


if __name__ == "__main__":
    main()
