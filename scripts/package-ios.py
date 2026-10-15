#!/usr/bin/env python3
"""Build a local unsigned iOS .app; signing and installation remain separate."""

import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import shlex
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--example", choices=tuple(g["id"] for g in json.loads((ROOT / "examples" / "catalog.json").read_text())), default="physics-2d")
    parser.add_argument("--target", choices=("aarch64-apple-ios", "aarch64-apple-ios-sim", "x86_64-apple-ios"),
                        default="aarch64-apple-ios-sim")
    parser.add_argument("--profile", choices=("release", "small"), default="release")
    parser.add_argument("--bundle-id", help="reverse-DNS identifier; defaults to dev.ferravex.<example>")
    parser.add_argument("--dry-run", action="store_true", help="print the plan without building or writing files")
    args = parser.parse_args()
    bundle_id = args.bundle_id or "dev.ferravex." + args.example.replace("-", "")
    if not re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+", bundle_id):
        parser.error("bundle-id must use reverse-DNS labels with letters, digits, or hyphens")
    minimum_os = os.environ.get("IPHONEOS_DEPLOYMENT_TARGET", "15.0")
    if not re.fullmatch(r"\d+\.\d+(?:\.\d+)?", minimum_os):
        parser.error("IPHONEOS_DEPLOYMENT_TARGET must be a version such as 15.0")
    command = ["cargo", "build", "--locked", "--profile", args.profile,
               "--target", args.target, "-p", args.example, "--bin", args.example]
    output = ROOT / "dist" / "ios" / args.target / args.profile / f"{args.example}.app"
    with (ROOT / "scripts" / "ios" / "Info.plist").open("rb") as file:
        info = plistlib.load(file)
    info.update({"CFBundleExecutable": args.example, "CFBundleName": args.example,
                 "CFBundleIdentifier": bundle_id, "MinimumOSVersion": minimum_os,
                 "CFBundleSupportedPlatforms": ["iPhoneOS" if args.target == "aarch64-apple-ios" else "iPhoneSimulator"]})
    print(f"IPHONEOS_DEPLOYMENT_TARGET={minimum_os} {shlex.join(command)}")
    print(f"Bundle: {output}\nIdentifier: {bundle_id}")
    if args.dry_run:
        print("Dry run: would copy the compiled executable and generate Info.plist; no files written.")
        return
    if sys.platform != "darwin":
        parser.error("building iOS requires macOS with Xcode; use --dry-run to inspect the plan elsewhere")
    if shutil.which("xcrun") is None:
        parser.error("xcrun is missing; install and select a full Xcode installation")
    subprocess.run(command, cwd=ROOT, env={**os.environ, "IPHONEOS_DEPLOYMENT_TARGET": minimum_os}, check=True)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT, text=True,
    ))
    binary = Path(metadata["target_directory"]) / args.target / args.profile / args.example
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, output / args.example)
    with (output / "Info.plist").open("wb") as file:
        plistlib.dump(info, file, sort_keys=False)
    print("Unsigned app created. Sign and install using your Xcode workflow before running it.")


if __name__ == "__main__":
    main()
