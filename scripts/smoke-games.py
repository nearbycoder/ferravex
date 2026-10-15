#!/usr/bin/env python3
"""Exercise first-party native simulations and their original PNG exporters.

Runs the original Linux GPU host for restored games, and the original headless
hosts for Courtyard and Orbit Breaker. Requires DISPLAY for windowed examples.
This is an integration smoke check, not a frame-rate benchmark.
"""
import argparse
import json
from pathlib import Path
import struct
import zlib
import hashlib
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release", "small"), default="release")
    parser.add_argument("--frames", type=int, default=720)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--game", action="append", help="only this game; repeat for a subset")
    parser.add_argument("--output", type=Path, default=ROOT / "target" / "game-smoke")
    args = parser.parse_args()
    if args.frames < 1 or args.timeout < 1:
        parser.error("frames and timeout must be positive")
    games = [g["id"] for g in json.loads((ROOT / "examples" / "catalog.json").read_text())]
    if args.game:
        unknown = set(args.game) - set(games)
        if unknown:
            parser.error("unknown games: " + ", ".join(sorted(unknown)))
        games = [g for g in games if g in args.game]
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT, text=True))
    binaries = Path(metadata["target_directory"]) / args.profile
    args.output.mkdir(parents=True, exist_ok=True)
    results = []
    for game in games:
        executable = binaries / game
        if not executable.is_file():
            executable = executable.with_suffix(".exe")
        screenshot = (args.output / f"{game}.png").resolve()
        # A fresh output is required: a previous successful run cannot mask a failure.
        screenshot.unlink(missing_ok=True)
        started = time.monotonic()
        result = {"game": game, "frames": args.frames, "passed": False}
        try:
            if game == 'owned-lab': options = ['--frames', str(args.frames), '--output', str(screenshot)]
            elif game == 'owned-orbit': options = ['--autoplay', '--frames', str(args.frames), '--screenshot', str(screenshot)]
            elif game == 'strike': options = ['--play', '--smoke-frames', str(args.frames), '--screenshot', str(screenshot)]
            else: options = ['--autoplay', '--smoke-frames', str(args.frames), '--screenshot', str(screenshot)]
            run = subprocess.run([str(executable), *options], cwd=ROOT, capture_output=True, text=True, timeout=args.timeout)
            result.update(exit_code=run.returncode, stdout=run.stdout, stderr=run.stderr)
            if run.returncode != 0:
                raise ValueError(f"exit {run.returncode}: {run.stderr[-1500:]}")
            header = screenshot.read_bytes()[:24]
            if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
                raise ValueError("missing or invalid PNG capture")
            width, height = struct.unpack(">II", header[16:24])
            if min(width, height) < 100:
                raise ValueError("capture is too small")
            data = screenshot.read_bytes()
            offset = 8
            compressed = bytearray()
            while offset < len(data):
                length = int.from_bytes(data[offset:offset + 4], 'big')
                kind = data[offset + 4:offset + 8]
                chunk = data[offset + 8:offset + 8 + length]
                checksum = int.from_bytes(data[offset + 8 + length:offset + 12 + length], 'big')
                if zlib.crc32(kind + chunk) != checksum:
                    raise ValueError("invalid PNG chunk checksum")
                if kind == b'IDAT': compressed.extend(chunk)
                offset += length + 12
            raw = zlib.decompress(compressed)
            stride = width * 4 + 1
            if len(raw) != stride * height or any(raw[y * stride] for y in range(height)):
                raise ValueError("unexpected original PNG scanline layout")
            colors = {raw[y * stride + 1 + x * 4:y * stride + 5 + x * 4]
                      for y in range(0, height, 7) for x in range(0, width, 7)}
            if len(colors) < 16:
                raise ValueError("blank or incomplete GPU capture")
            result.update(passed=True, screenshot=str(screenshot.relative_to(ROOT)) if screenshot.is_relative_to(ROOT) else str(screenshot), width=width, height=height, sampled_colors=len(colors), sha256=hashlib.sha256(data).hexdigest())
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            result["error"] = str(error)
        result["wall_seconds"] = round(time.monotonic() - started, 3)
        results.append(result)
        print(f'{"PASS" if result["passed"] else "FAIL"} {game}: {result.get("error", result.get("stdout", "").strip())}', flush=True)
    report = {"profile": args.profile, "note": "Local simulation, rendering and PNG export checks, not performance measurements", "results": results}
    (args.output / "smoke-results.json").write_text(json.dumps(report, indent=2) + "\n")
    raise SystemExit(0 if all(r["passed"] for r in results) else 1)

if __name__ == "__main__":
    main()
