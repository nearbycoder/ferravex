#!/usr/bin/env python3
"""Serve the staged browser demos with Python's standard library."""

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Handler(SimpleHTTPRequestHandler):
    extensions_map = {**SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm"}

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--directory", type=Path, default=ROOT / "dist" / "web")
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error("port must be between 1 and 65535")
    directory = args.directory.resolve()
    if not (directory / "index.html").is_file():
        parser.error(f"no index.html in {directory}; run scripts/build-owned.py or scripts/build-web.sh first")
    with ThreadingHTTPServer((args.bind, args.port), partial(Handler, directory=str(directory))) as server:
        entry = "owned/" if (directory / "owned" / "index.html").is_file() else "?demo=strike"
        print(f"Ferravex demos: http://{args.bind}:{args.port}/{entry}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
