#!/usr/bin/env python3
"""Build every portable example in the game catalog."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=("release", "small"), default="release", nargs="?")
    args = parser.parse_args()
    subprocess.run(["python3", "scripts/build-owned.py", "--profile", args.profile], cwd=ROOT, check=True)

if __name__ == "__main__":
    main()
