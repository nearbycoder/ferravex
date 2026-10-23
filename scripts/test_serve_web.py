"""Local HTTP regressions using only temporary fixtures and stdlib servers."""

from contextlib import redirect_stderr, redirect_stdout
import errno
from functools import partial
import hashlib
import http.client
import importlib.util
import io
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import MagicMock, patch


spec = importlib.util.spec_from_file_location("serve_web", Path(__file__).with_name("serve-web.py"))
web = importlib.util.module_from_spec(spec)
spec.loader.exec_module(web)


class QuietHandler(web.Handler):
    def log_message(self, *args):
        pass


class HttpTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory(prefix="ferravex serve web ")
        cls.addClassCleanup(temporary.cleanup)
        cls.root = Path(temporary.name)
        cls.index = b"<h1>Demo catalog</h1>"
        cls.asset = bytes(range(256)) * 1025
        cls.module = b"\x00asm\x01\x00\x00\x00"
        (cls.root / "index.html").write_bytes(cls.index)
        (cls.root / "owned").mkdir()
        (cls.root / "owned" / "index.html").write_bytes(b"Courtyard")
        (cls.root / "listing").mkdir()
        (cls.root / "legacy").mkdir()
        (cls.root / "legacy" / "index.htm").write_bytes(b"Legacy index")
        (cls.root / "game.wasm").write_bytes(cls.asset)
        (cls.root / "minimal.wasm").write_bytes(cls.module)
        (cls.root / "empty.wasm").write_bytes(b"")
        for name in ("module.JS", "module.mjs", "theme.css", "data.json", "code.map",
                     "module.WASM", "space name.wasm", "unknown.ferravex_unknown"):
            (cls.root / name).write_bytes(b"fixture")
        cls.server = web.ThreadingHTTPServer(
            ("127.0.0.1", 0), partial(QuietHandler, directory=str(cls.root))
        )
        cls.addClassCleanup(cls.server.server_close)
        cls.thread = threading.Thread(
            target=cls.server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True
        )
        cls.thread.start()
        cls.addClassCleanup(cls.thread.join, 5)
        cls.addClassCleanup(cls.server.shutdown)

    def request(self, path="/game.wasm", headers=None, method="GET", server=None):
        server = server or self.server
        connection = http.client.HTTPConnection(
            server.server_address[0], server.server_port, timeout=5
        )
        try:
            connection.request(method, path, headers=headers or {})
            response = connection.getresponse()
            result = response.status, dict(response.getheaders()), response.read()
            self.assertEqual(response.headers.get_all("Cache-Control"), ["no-cache"])
            self.assertEqual(response.headers["X-Content-Type-Options"], "nosniff")
            return result
        finally:
            connection.close()

    def test_default_routes_redirects_and_listings_are_preserved(self):
        for path, expected in (("/", self.index), ("/owned/", b"Courtyard"),
                               ("/legacy/", b"Legacy index"), ("/owned%2F", b"Courtyard")):
            with self.subTest(path=path):
                status, headers, body = self.request(path)
                self.assertEqual((status, body), (200, expected))
                self.assertIn("ETag", headers)
        status, headers, body = self.request("/owned?autoplay=1")
        self.assertEqual((status, body), (301, b""))
        self.assertEqual(headers["Location"], "/owned/?autoplay=1")
        self.assertEqual(self.request("/listing/")[0], 200)

    def test_mime_types_are_independent_of_machine_mime_database(self):
        expected = {
            "/minimal.wasm?v=2": "application/wasm",
            "/module.WASM": "application/wasm",
            "/space%20name.wasm": "application/wasm",
            "/module.JS": "text/javascript",
            "/module.mjs": "text/javascript",
            "/theme.css": "text/css",
            "/data.json": "application/json",
            "/code.map": "application/json",
            "/": "text/html",
            "/unknown.ferravex_unknown": "application/octet-stream",
        }
        for path, content_type in expected.items():
            with self.subTest(path=path):
                status, headers, body = self.request(path)
                self.assertEqual(status, 200)
                self.assertEqual(headers["Content-Type"], content_type)
                self.assertEqual(int(headers["Content-Length"]), len(body))

    def test_full_head_and_empty_file_metadata(self):
        status, headers, body = self.request(headers={"Accept-Encoding": "gzip, br"})
        self.assertEqual((status, body), (200, self.asset))
        self.assertEqual(headers["Accept-Ranges"], "bytes")
        self.assertNotIn("Content-Encoding", headers)
        self.assertNotIn("Content-Range", headers)
        self.assertEqual(headers["ETag"], '"' + hashlib.sha256(self.asset).hexdigest() + '"')
        status, head_headers, body = self.request(method="HEAD", headers={"Range": "bytes=0-7"})
        self.assertEqual((status, body), (200, b""))
        for name in ("Content-Length", "Content-Type", "ETag", "Accept-Ranges"):
            self.assertEqual(head_headers[name], headers[name])
        self.assertNotIn("Content-Range", head_headers)
        status, headers, body = self.request("/empty.wasm")
        self.assertEqual((status, headers["Content-Length"], body), (200, "0", b""))

    def test_single_ranges_closed_open_suffix_and_clamping(self):
        size = len(self.asset)
        for value, start, end in (
            ("bytes=0-7", 0, 7),
            ("bytes=42-42", 42, 42),
            ("bytes=1-131080", 1, 131080),
            (f"bytes={size - 4}-", size - 4, size - 1),
            ("bytes=-8", size - 8, size - 1),
            (f"bytes=1-{size + 1}", 1, size - 1),
            (f"bytes=-{size + 1}", 0, size - 1),
            ("Bytes=0-0", 0, 0),
        ):
            with self.subTest(value=value):
                status, headers, body = self.request(headers={"Range": value})
                self.assertEqual(status, 206)
                self.assertEqual(headers["Content-Range"], f"bytes {start}-{end}/{size}")
                self.assertEqual(int(headers["Content-Length"]), end - start + 1)
                self.assertEqual(body, self.asset[start:end + 1])
                self.assertEqual(headers["Content-Type"], "application/wasm")

    def test_unsatisfiable_ranges_have_size_and_empty_body(self):
        for path, value, size in (
            ("/game.wasm", f"bytes={len(self.asset)}-", len(self.asset)),
            ("/game.wasm", "bytes=-0", len(self.asset)),
            ("/game.wasm", "bytes=" + "9" * 200 + "-", len(self.asset)),
            ("/empty.wasm", "bytes=0-0", 0),
            ("/empty.wasm", "bytes=-1", 0),
        ):
            with self.subTest(path=path, value=value):
                status, headers, body = self.request(path, {"Range": value})
                self.assertEqual((status, body), (416, b""))
                self.assertEqual(headers["Content-Range"], f"bytes */{size}")
                self.assertEqual(headers["Content-Length"], "0")

    def test_unsupported_or_invalid_ranges_fall_back_to_full_response(self):
        for value in ("items=0-1", "bytes=0-1,4-5", "bytes=-", "bytes=5-2",
                      "bytes=one-two", "bytes=+1-2", "bytes=²-³", "bytes=0--1",
                      "bytes=" + "9" * 5000 + "-", "bytes=0-1 trailing"):
            with self.subTest(value=value[:40]):
                status, headers, body = self.request(headers={"Range": value})
                self.assertEqual((status, body), (200, self.asset))
                self.assertNotIn("Content-Range", headers)

    def test_etag_revalidation_precedes_range_and_supports_weak_lists_and_head(self):
        etag = self.request()[1]["ETag"]
        for value in (etag, "W/" + etag, '"old", W/' + etag, "*"):
            for method in ("GET", "HEAD"):
                with self.subTest(value=value, method=method):
                    status, headers, body = self.request(
                        headers={"If-None-Match": value, "Range": "bytes=999999-"}, method=method
                    )
                    self.assertEqual((status, body), (304, b""))
                    self.assertEqual(headers["ETag"], etag)
                    self.assertNotIn("Content-Range", headers)
                    self.assertNotIn("Content-Length", headers)
        self.assertEqual(self.request(headers={"If-None-Match": '"old"'})[0], 200)

    def test_rebuild_with_same_size_and_timestamp_changes_etag(self):
        path = self.root / "rebuild.wasm"
        path.write_bytes(b"old!")
        original = path.stat()
        etag = self.request("/rebuild.wasm")[1]["ETag"]
        path.write_bytes(b"new!")
        os.utime(path, ns=(original.st_atime_ns, original.st_mtime_ns))
        status, headers, body = self.request(
            "/rebuild.wasm", {
                "If-None-Match": etag,
                "If-Modified-Since": "Fri, 01 Jan 2100 00:00:00 GMT",
            }
        )
        self.assertEqual((status, body), (200, b"new!"))
        self.assertNotEqual(headers["ETag"], etag)

    def test_asset_changed_during_hash_returns_retryable_error(self):
        path = self.root / "building.wasm"
        path.write_bytes(self.module)
        digest = hashlib.sha256()
        hasher = MagicMock(wraps=digest)

        def update_and_grow(chunk):
            digest.update(chunk)
            with path.open("ab") as stream:
                stream.write(b"new build")

        hasher.update.side_effect = update_and_grow
        with patch.object(web.hashlib, "sha256", return_value=hasher):
            status, headers, body = self.request("/building.wasm")
        self.assertEqual(status, 503)
        self.assertIn(b"retry after the build finishes", body)
        self.assertNotIn("ETag", headers)
        self.assertEqual(self.request("/building.wasm")[::2], (200, self.module + b"new build"))

    def test_date_conditions_do_not_reuse_a_potentially_stale_build(self):
        status, headers, body = self.request(
            headers={"If-Modified-Since": "Fri, 01 Jan 2100 00:00:00 GMT"}
        )
        self.assertEqual((status, body), (200, self.asset))
        self.assertNotIn("Last-Modified", headers)

    def test_if_range_requires_current_strong_etag(self):
        etag = self.request()[1]["ETag"]
        for validator, expected in ((etag, 206), ('"old"', 200), ("W/" + etag, 200),
                                    ("Fri, 01 Jan 2100 00:00:00 GMT", 200)):
            with self.subTest(validator=validator):
                status, _, body = self.request(
                    headers={"Range": "bytes=0-7", "If-Range": validator}
                )
                self.assertEqual(status, expected)
                self.assertEqual(body, self.asset[:8] if expected == 206 else self.asset)

    def test_if_match_is_strong_and_precedes_other_conditions(self):
        etag = self.request()[1]["ETag"]
        for value in ('"old"', "W/" + etag):
            status, _, _ = self.request(headers={"If-Match": value, "If-None-Match": etag})
            self.assertEqual(status, 412)
        for value in (etag, '"old", ' + etag, "*"):
            self.assertEqual(self.request(headers={"If-Match": value})[0], 200)

    def test_missing_assets_are_errors_and_new_files_become_visible(self):
        status, headers, body = self.request("/new.wasm", {"If-None-Match": "*"})
        self.assertEqual(status, 404)
        self.assertIn(b"scripts/build-web.py", body)
        self.assertTrue(headers["Content-Type"].startswith("text/html"))
        (self.root / "new.wasm").write_bytes(self.module)
        self.assertEqual(self.request("/new.wasm")[::2], (200, self.module))
        for path, expected in (("/game.wasm/", 404), ("/bad%00name", 400),
                               ("/../../not-staged.wasm", 404)):
            self.assertEqual(self.request(path)[0], expected)
        status, _, body = self.request("/missing.wasm", method="HEAD")
        self.assertEqual((status, body), (404, b""))

    def test_permission_and_io_errors_have_explicit_responses(self):
        for error, expected in ((PermissionError("denied"), 403), (OSError("read failed"), 500)):
            with patch("builtins.open", side_effect=error):
                self.assertEqual(self.request()[0], expected)

    @unittest.skipUnless(hasattr(os, "mkfifo"), "FIFO fixtures require POSIX")
    def test_non_regular_asset_does_not_block(self):
        os.mkfifo(self.root / "pipe.wasm")
        self.assertEqual(self.request("/pipe.wasm")[0], 404)

    def test_node_fetch_can_instantiate_streaming_wasm(self):
        node = shutil.which("node")
        if node is None:
            self.skipTest("Node is not installed; stdlib HTTP tests still run")
        result = subprocess.run(
            [node, "--input-type=module", "--eval", """
                import assert from 'node:assert/strict';
                const url = process.argv[1] + '/minimal.wasm';
                const response = await fetch(url);
                assert.equal(response.status, 200);
                assert.equal(response.headers.get('content-type'), 'application/wasm');
                const {instance} = await WebAssembly.instantiateStreaming(response);
                assert.ok(instance instanceof WebAssembly.Instance);
                const partial = await fetch(url, {headers: {Range: 'bytes=0-3'}});
                assert.equal(partial.status, 206);
                const bytes = new Uint8Array(await partial.arrayBuffer());
                assert.deepEqual([...bytes], [0, 97, 115, 109]);
            """, f"http://127.0.0.1:{self.server.server_port}"],
            capture_output=True, text=True, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_ipv6_loopback_requests_when_available(self):
        try:
            server = web._IPv6Server(("::1", 0), partial(QuietHandler, directory=str(self.root)))
        except OSError as error:
            self.skipTest(f"IPv6 loopback unavailable: {error}")
        with server:
            thread = threading.Thread(
                target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True
            )
            thread.start()
            try:
                self.assertEqual(self.request("/", server=server)[::2], (200, self.index))
            finally:
                server.shutdown()
                thread.join(5)


class CliTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="ferravex serve cli ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "index.html").write_bytes(b"index")

    def invoke(self, *args):
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            try:
                status = web.main(["--directory", str(self.root), *args])
            except SystemExit as error:
                status = error.code
        return status, output.getvalue(), errors.getvalue()

    def fake_server(self, address):
        server = MagicMock()
        server.server_address = address
        server.__enter__.return_value = server
        server.serve_forever.side_effect = KeyboardInterrupt
        return server

    def test_defaults_report_loopback_catalog_and_existing_demo(self):
        server = self.fake_server(("127.0.0.1", 8000))
        with patch.object(web, "ThreadingHTTPServer", return_value=server) as factory:
            status, output, errors = self.invoke()
        self.assertIsNone(status)
        self.assertEqual(errors, "")
        self.assertEqual(factory.call_args.args[0], ("127.0.0.1", 8000))
        self.assertIn("loopback only", output)
        self.assertIn("Ferravex catalog: http://127.0.0.1:8000/", output)
        self.assertIn("Ferravex demos: http://127.0.0.1:8000/owned/", output)
        self.assertIn(str(self.root), output)
        server.__exit__.assert_called_once()

    def test_wildcard_reports_usable_local_url_and_network_exposure(self):
        server = self.fake_server(("0.0.0.0", 8123))
        with patch.object(web, "ThreadingHTTPServer", return_value=server):
            status, output, errors = self.invoke("--bind", "0.0.0.0", "--port", "8123")
        self.assertIsNone(status)
        self.assertIn("network accessible", output)
        self.assertIn("http://127.0.0.1:8123/", output)
        self.assertNotIn("http://0.0.0.0", output)
        self.assertIn("Non-loopback bind", errors)
        self.assertIn("--bind 127.0.0.1", errors)

    def test_ipv6_diagnostics_use_bracketed_url_and_ipv6_server(self):
        addresses = [(socket.AF_INET6, socket.SOCK_STREAM, 6, "", ("::1", 8123, 0, 0))]
        server = self.fake_server(("::1", 8123, 0, 0))
        with (patch.object(web.socket, "getaddrinfo", return_value=addresses),
              patch.object(web, "_IPv6Server", return_value=server)):
            status, output, errors = self.invoke("--bind", "::1", "--port", "8123")
        self.assertIsNone(status)
        self.assertEqual(errors, "")
        self.assertIn("loopback only", output)
        self.assertIn("http://[::1]:8123/owned/", output)

    def test_bad_ports_and_empty_bind_fail_before_listening(self):
        for args in (("--port", "0"), ("--port", "65536"), ("--port", "oops"), ("--bind", "")):
            with self.subTest(args=args), patch.object(web, "ThreadingHTTPServer") as factory:
                status, _, errors = self.invoke(*args)
                self.assertEqual(status, 2)
                self.assertIn("error:", errors)
                factory.assert_not_called()

    def test_missing_index_has_current_build_command(self):
        (self.root / "index.html").unlink()
        status, _, errors = self.invoke()
        self.assertEqual(status, 2)
        self.assertIn("no index.html", errors)
        self.assertIn("python3 -B scripts/build-web.py", errors)
        self.assertIn("--directory", errors)

    def test_unreadable_index_is_reported_without_traceback(self):
        with patch.object(Path, "open", side_effect=PermissionError("fixture denied")):
            status, _, errors = self.invoke()
        self.assertEqual(status, 2)
        self.assertIn("cannot read staged index", errors)
        self.assertNotIn("Traceback", errors)

    def test_real_occupied_port_is_actionable(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            status, _, errors = self.invoke("--port", str(listener.getsockname()[1]))
        self.assertEqual(status, 2)
        self.assertIn("port already in use", errors)
        self.assertIn("another --port", errors)
        self.assertNotIn("Traceback", errors)

    def test_resolution_and_bind_permissions_are_actionable(self):
        with patch.object(web.socket, "getaddrinfo", side_effect=socket.gaierror("unknown host")):
            status, _, errors = self.invoke("--bind", "invalid.example")
        self.assertEqual(status, 2)
        self.assertIn("cannot listen on invalid.example:8000", errors)
        self.assertIn("--bind 127.0.0.1", errors)
        with patch.object(
            web, "ThreadingHTTPServer", side_effect=PermissionError(errno.EACCES, "denied")
        ):
            status, _, errors = self.invoke()
        self.assertEqual(status, 2)
        self.assertIn("unprivileged --port", errors)

    def test_serve_failure_closes_server_and_reports_error(self):
        server = self.fake_server(("127.0.0.1", 8000))
        server.serve_forever.side_effect = OSError("fixture failure")
        with patch.object(web, "ThreadingHTTPServer", return_value=server):
            status, _, errors = self.invoke()
        self.assertEqual(status, 2)
        self.assertIn("local web server failed", errors)
        server.__exit__.assert_called_once()

    def test_script_entry_point_help_runs_without_build(self):
        result = subprocess.run(
            [sys.executable, "-B", str(Path(web.__file__)), "--help"],
            capture_output=True, text=True, timeout=5,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("loopback", result.stdout)


if __name__ == "__main__":
    unittest.main()
