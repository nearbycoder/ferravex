#!/usr/bin/env python3
"""Build every portable example in the game catalog."""
import argparse
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def main(argv: list[str] | None = None) -> int:
    """Run the owned builder with this interpreter and preserve failure status."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=("release", "small"), default="release", nargs="?")
    args = parser.parse_args(argv)
    try:
        subprocess.run(
            [sys.executable, "-B", "scripts/build-owned.py", "--profile", args.profile],
            cwd=ROOT, check=True,
        )
    except subprocess.CalledProcessError as error:
        return error.returncode if error.returncode > 0 else 1
    except OSError as error:
        print(f"web build failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
