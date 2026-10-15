#!/usr/bin/env python3
"""Exercise the public static server and check every generated guide/example link."""
import functools
import gzip
import http.client
import importlib.util
import re
import tempfile
import threading
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('site_host', ROOT / 'scripts/serve-site.py')
host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host)

class QuietHandler(host.Handler):
    def log_message(self, *args):
        pass

class SiteTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        directory = Path(cls.temp.name)
        (directory / 'index.html').write_bytes(b'<h1>Ferravex</h1>')
        (directory / 'game.wasm').write_bytes(bytes(range(128)))
        (directory / 'game.wasm.gz').write_bytes(gzip.compress(bytes(range(128))))
        (directory / 'empty').mkdir()
        cls.server = host.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(QuietHandler, directory=str(directory)))
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.thread.join()
        cls.server.server_close()
        cls.temp.cleanup()

    def request(self, path, headers=None, method='GET'):
        connection = http.client.HTTPConnection('127.0.0.1', self.server.server_port, timeout=5)
        connection.request(method, path, headers=headers or {})
        response = connection.getresponse()
        result = response.status, dict(response.getheaders()), response.read()
        connection.close()
        return result

    def test_health_and_head(self):
        self.assertEqual(self.request('/healthz')[0], 200)
        status, headers, body = self.request('/', method='HEAD')
        self.assertEqual(status, 200)
        self.assertEqual(int(headers['Content-Length']), len(b'<h1>Ferravex</h1>'))
        self.assertEqual(body, b'')

    def test_gzip_and_conditional(self):
        status, headers, body = self.request('/game.wasm', {'Accept-Encoding': 'gzip'})
        self.assertEqual(status, 200)
        self.assertEqual(headers['Content-Type'], 'application/wasm')
        self.assertEqual(headers['Content-Encoding'], 'gzip')
        self.assertEqual(gzip.decompress(body), bytes(range(128)))
        status, _, body = self.request('/game.wasm', {'Accept-Encoding': 'gzip', 'If-None-Match': headers['ETag']})
        self.assertEqual((status, body), (304, b''))

    def test_byte_ranges(self):
        for selection, expected in [('0-7', bytes(range(8))), ('120-', bytes(range(120, 128))), ('-8', bytes(range(120, 128)))]:
            with self.subTest(selection=selection):
                status, headers, body = self.request('/game.wasm', {'Range': 'bytes=' + selection, 'Accept-Encoding': 'gzip'})
                self.assertEqual(status, 206)
                self.assertNotIn('Content-Encoding', headers)
                self.assertEqual(body, expected)
                self.assertEqual(int(headers['Content-Length']), len(expected))
        for selection in ['128-', '5-2', '-', '0-1,4-5']:
            self.assertEqual(self.request('/game.wasm', {'Range': 'bytes=' + selection})[0], 416)

    def test_no_listing_or_path_escape(self):
        self.assertEqual(self.request('/empty')[0], 308)
        self.assertEqual(self.request('/empty/')[0], 404)
        self.assertEqual(self.request('/../../etc/passwd')[0], 404)
        self.assertEqual(self.request('/%2e%2e/%2e%2e/etc/passwd')[0], 404)

    def test_generated_links_and_catalog(self):
        root = ROOT / 'dist/site'
        self.assertTrue((root / 'index.html').is_file(), 'Build the documentation site first')
        pages = [root / 'index.html', *root.glob('docs/**/*.html'), *root.glob('examples/**/*.html')]
        self.assertEqual(len(list((root / 'examples').glob('*/index.html'))), 15)
        for page in pages:
            for value in re.findall(r'(?:href|src)="([^"]+)"', page.read_text()):
                url = urlsplit(value)
                if url.scheme or url.netloc or not url.path:
                    continue
                path = root / unquote(url.path.lstrip('/')) if url.path.startswith('/') else page.parent / unquote(url.path)
                if path.is_dir():
                    path = path / 'index.html'
                self.assertTrue(path.is_file(), f'{page.relative_to(root)}: {value}')

if __name__ == '__main__':
    unittest.main()
