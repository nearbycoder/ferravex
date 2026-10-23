#!/usr/bin/env python3
"""Serve staged browser demos locally with Python's standard library.

Run ``python3 -B scripts/serve-web.py`` after ``scripts/build-web.py``.
The catalog stays at / and Courtyard at /owned/. File responses use content
ETags and no-cache so even same-size, same-timestamp rebuilds are revalidated.
Date validators are deliberately not used: build tools can preserve timestamps
or replace an asset more than once per second. If-Range requires a strong ETag.
Single byte ranges are supported; malformed, multiple, or unknown ranges fall
back to a full response. Files are hashed and copied in bounded chunks.
"""

import argparse
import errno
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import ipaddress
import os
from pathlib import Path
import re
import socket
import stat
import sys
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent
COPY_CHUNK_SIZE = 64 * 1024


def _byte_range(value, size):
    """Return inclusive bounds, None to ignore, or raise for an empty selection."""
    # Ignore overly complex input before converting arbitrary-length integers.
    if len(value) > 256:
        return None
    match = re.fullmatch(r"bytes=([0-9]*)-([0-9]*)", value.strip(), re.IGNORECASE)
    if not match or not any(match.groups()):
        return None
    first, last = match.groups()
    if first:
        start = int(first)
        end = int(last) if last else size - 1
        if last and end < start:
            return None
        if start >= size:
            raise ValueError("range starts beyond the file")
        return start, min(end, size - 1)
    length = int(last)
    if not length or not size:
        raise ValueError("range selects no bytes")
    return max(0, size - length), size - 1


def _etag_matches(value, etag, *, weak):
    if value.strip() == "*":
        return True
    tags = (tag.strip() for tag in value.split(","))
    return any((tag.removeprefix("W/") if weak else tag) == etag for tag in tags)


class Handler(SimpleHTTPRequestHandler):
    """Static GET/HEAD handler; accepts the stdlib's ``directory=`` argument."""

    extensions_map = {
        **SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".css": "text/css",
        ".html": "text/html",
        ".json": "application/json",
        ".map": "application/json",
    }

    def end_headers(self):
        self.send_header("Cache-Control", "no-cache")
        self.send_header("X-Content-Type-Options", "nosniff")
        super().end_headers()

    def send_head(self):
        self._remaining = None
        path = self.translate_path(self.path)
        if os.path.isdir(path):
            if not urlsplit(self.path).path.endswith(("/", "%2f", "%2F")):
                return super().send_head()
            for index in ("index.html", "index.htm"):
                candidate = os.path.join(path, index)
                if os.path.isfile(candidate):
                    path = candidate
                    break
            else:
                return self.list_directory(path)
        if path.endswith("/"):
            self.send_error(404, "File not found")
            return None

        stream = None
        try:
            # A FIFO/device is not a staged asset and must not block a worker.
            if not stat.S_ISREG(os.stat(path).st_mode):
                self.send_error(404, "Not a regular file")
                return None
            stream = open(path, "rb")
            before = os.fstat(stream.fileno())
            size = before.st_size
            digest = hashlib.sha256()
            remaining = size
            while remaining:
                chunk = stream.read(min(remaining, COPY_CHUNK_SIZE))
                if not chunk:
                    break
                digest.update(chunk)
                remaining -= len(chunk)
            after = os.fstat(stream.fileno())
            if (remaining or before.st_size != after.st_size
                    or before.st_mtime_ns != after.st_mtime_ns
                    or before.st_ctime_ns != after.st_ctime_ns):
                stream.close()
                self.send_error(
                    503, "Staged asset changed while reading; retry after the build finishes"
                )
                return None
            stream.seek(0)
        except PermissionError:
            if stream is not None:
                stream.close()
            self.send_error(403, "Cannot read staged asset; check file permissions")
            return None
        except (OSError, ValueError) as error:
            if stream is not None:
                stream.close()
            if isinstance(error, (FileNotFoundError, NotADirectoryError)):
                self.send_error(
                    404, "Staged asset not found; check the URL and rerun scripts/build-web.py"
                )
            elif isinstance(error, ValueError):
                self.send_error(400, "Invalid asset path")
            else:
                self.log_error("Cannot read staged asset: %s", error)
                self.send_error(500, "Cannot read staged asset; see server log")
            return None

        try:
            etag = '"' + digest.hexdigest() + '"'
            if_match = ",".join(self.headers.get_all("If-Match", []))
            if if_match and not _etag_matches(if_match, etag, weak=False):
                stream.close()
                self.send_error(412, "Asset changed; fetch it again before requesting a range")
                return None
            if_none_match = ",".join(self.headers.get_all("If-None-Match", []))
            if if_none_match and _etag_matches(if_none_match, etag, weak=True):
                stream.close()
                self.send_response(304)
                self.send_header("ETag", etag)
                self.end_headers()
                return None

            selection = None
            range_header = ",".join(self.headers.get_all("Range", []))
            # Date/weak If-Range validators cannot establish unchanged bytes.
            if (self.command == "GET" and range_header
                    and self.headers.get("If-Range", etag).strip() == etag):
                try:
                    selection = _byte_range(range_header, size)
                except ValueError:
                    stream.close()
                    self.send_response(416)
                    self.send_header("Content-Range", f"bytes */{size}")
                    self.send_header("Content-Length", "0")
                    self.send_header("Accept-Ranges", "bytes")
                    self.send_header("ETag", etag)
                    self.end_headers()
                    return None

            start, end = selection if selection is not None else (0, size - 1)
            self._remaining = end - start + 1
            stream.seek(start)
            self.send_response(206 if selection is not None else 200)
            self.send_header("Content-Type", self.guess_type(path))
            self.send_header("Content-Length", str(self._remaining))
            self.send_header("Accept-Ranges", "bytes")
            self.send_header("ETag", etag)
            if selection is not None:
                self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
            self.end_headers()
            return stream
        except BaseException:
            stream.close()
            raise

    def copyfile(self, source, outputfile):
        if self._remaining is None:
            return super().copyfile(source, outputfile)
        while self._remaining:
            chunk = source.read(min(self._remaining, COPY_CHUNK_SIZE))
            if not chunk:
                break
            outputfile.write(chunk)
            self._remaining -= len(chunk)


class _IPv6Server(ThreadingHTTPServer):
    address_family = socket.AF_INET6


def _loopback(address):
    ip = ipaddress.ip_address(address.split("%", 1)[0])
    mapped = getattr(ip, "ipv4_mapped", None)
    return (mapped or ip).is_loopback


def main(argv=None):
    """Serve until interrupted; optional argv allows isolated CLI tests."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument(
        "--bind", default="127.0.0.1", help="listen address (default: loopback only)"
    )
    parser.add_argument("--directory", type=Path, default=ROOT / "dist" / "web")
    args = parser.parse_args(argv)
    if not 1 <= args.port <= 65535:
        parser.error("port must be between 1 and 65535")
    try:
        directory = args.directory.resolve()
        if not (directory / "index.html").is_file():
            parser.error(
                f"no index.html in {directory}; run python3 -B scripts/build-web.py "
                "or select an existing build with --directory"
            )
        with (directory / "index.html").open("rb") as index:
            index.read(1)
    except (OSError, ValueError) as error:
        parser.error(f"cannot read staged index in {args.directory}: {error}")
    if not args.bind.strip():
        parser.error("--bind must name an address; use 127.0.0.1 for loopback")
    try:
        addresses = socket.getaddrinfo(args.bind, args.port, type=socket.SOCK_STREAM)
        # Keep IPv4 preference for existing hostname invocations such as localhost.
        family, _, _, _, address = min(
            addresses, key=lambda candidate: candidate[0] != socket.AF_INET
        )
        server_type = _IPv6Server if family == socket.AF_INET6 else ThreadingHTTPServer
        server = server_type(address, partial(Handler, directory=str(directory)))
    except OSError as error:
        if error.errno == errno.EADDRINUSE:
            hint = "port already in use; stop that server or choose another --port"
        elif error.errno in (errno.EACCES, errno.EPERM):
            hint = "permission denied; choose an unprivileged --port and check bind permissions"
        else:
            hint = "check --bind and --port; use --bind 127.0.0.1 for local testing"
        parser.error(f"cannot listen on {args.bind}:{args.port}: {error}; {hint}")
    with server:
        bound = server.server_address[0]
        port = server.server_address[1]
        local = _loopback(bound)
        wildcard = ipaddress.ip_address(bound.split("%", 1)[0]).is_unspecified
        browser_host = ("::1" if family == socket.AF_INET6 else "127.0.0.1") if wildcard else bound
        url_host = f"[{browser_host}]" if ":" in browser_host else browser_host
        base = f"http://{url_host}:{port}/"
        exposure = "loopback only" if local else "network accessible"
        print(f"Serving {directory} on {bound}:{port} ({exposure})", flush=True)
        print(f"Ferravex catalog: {base}", flush=True)
        print(f"Ferravex demos: {base}owned/", flush=True)
        if not local:
            print(
                "Non-loopback bind: other machines may reach these files. "
                "For local Wasm testing use --bind 127.0.0.1; "
                "browser features requiring a secure context may be unavailable over LAN HTTP.",
                file=sys.stderr, flush=True,
            )
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
        except OSError as error:
            parser.error(f"local web server failed: {error}")


if __name__ == "__main__":
    main()
