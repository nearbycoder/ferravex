#!/usr/bin/env python3
"""Build every portable example in the game catalog."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=("release", "small"), default="release", nargs="?")
    args = parser.parse_args()
    games = json.loads((ROOT / "examples" / "catalog.json").read_text())
    command = ["cargo", "build", "--locked", "--profile", args.profile,
               "--target", "wasm32-unknown-unknown", "--bins"]
    for game in games:
        command.extend(["-p", game["id"]])
    subprocess.run(command, cwd=ROOT, check=True)

if __name__ == "__main__":
    main()
