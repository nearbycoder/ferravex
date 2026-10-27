#!/usr/bin/env python3
"""Exercise the public static server and check every generated guide/example link."""
import json
import functools
import gzip
import http.client
import importlib.util
import re
import subprocess
import sys
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

    def test_missing_paths_serve_custom_not_found_page(self):
        directory = Path(self.temp.name)
        page = b'<h1>Out of bounds</h1>'
        (directory / '404.html').write_bytes(page)
        try:
            for path in ['/missing/', '/missing.png', '/empty/']:
                with self.subTest(path=path):
                    status, headers, body = self.request(path)
                    self.assertEqual((status, body), (404, page))
                    self.assertEqual(headers['Content-Type'], 'text/html; charset=utf-8')
                    self.assertEqual(headers['Cache-Control'], 'no-cache')
            status, headers, body = self.request('/missing/', method='HEAD')
            self.assertEqual((status, body, int(headers['Content-Length'])), (404, b'', len(page)))
            self.assertEqual(self.request('/../../etc/passwd')[2], page)
        finally:
            (directory / '404.html').unlink()

    def test_generated_links_and_catalog(self):
        root = ROOT / 'dist/site'
        self.assertTrue((root / 'index.html').is_file(), 'Build the documentation site first')
        pages = [root / 'index.html', *root.glob('docs/**/*.html'), *root.glob('examples/**/*.html')]
        self.assertEqual({p.parent.name for p in (root / 'examples').glob('*/index.html')}, {g['id'] for g in json.loads((ROOT / 'examples/catalog.json').read_text())})
        self.assertEqual(len(list((root / 'play').rglob('*.wasm'))), len(json.loads((ROOT / 'examples/catalog.json').read_text())))
        self.assertFalse((root / 'third-party').exists())
        self.assertTrue((root / 'api/ferravex_physics').exists())
        self.assertTrue((root / 'play/runtime.js').exists())
        self.assertFalse((root / 'play/mq_js_bundle.js').exists())
        for page in pages:
            for value in re.findall(r'(?:href|src)="([^"]+)"', page.read_text()):
                url = urlsplit(value)
                if url.scheme or url.netloc or not url.path:
                    continue
                path = root / unquote(url.path.lstrip('/')) if url.path.startswith('/') else page.parent / unquote(url.path)
                if path.is_dir():
                    path = path / 'index.html'
                self.assertTrue(path.is_file(), f'{page.relative_to(root)}: {value}')

    def test_example_goals_and_learning_paths(self):
        root = ROOT / 'dist/site'
        for game in json.loads((ROOT / 'examples/catalog.json').read_text()):
            with self.subTest(game=game['id']):
                page = (root / 'examples' / game['id'] / 'index.html').read_text()
                self.assertIn('Your objective', page)
                self.assertIn('Default controls', page)
                self.assertIn('Open full player', page)
                self.assertIn('class="play-brief"', page)
                self.assertLess(page.index('Your objective'), page.index('data-launch'))
                self.assertRegex(page, r'href="/docs/[a-z-]+/">Explore:')

    def test_every_game_page_has_gameplay_loop_poster_and_stills(self):
        root = ROOT / 'dist/site'
        for game in json.loads((ROOT / 'examples/catalog.json').read_text()):
            with self.subTest(game=game['id']):
                page = (root / 'examples' / game['id'] / 'index.html').read_text()
                loop, poster = f"/media/{game['id']}/loop.mp4", f"/media/{game['id']}/poster.webp"
                self.assertIn(f'data-loop="{loop}"', page)
                self.assertIn(f'src="{poster}"', page)
                for url in (loop, poster):
                    self.assertTrue((root / url.lstrip('/')).is_file(), url)
                self.assertGreaterEqual(page.count('data-lightbox'), 3)
                # Loops are created by site.js only when allowed; markup never autoplays or preloads video.
                self.assertNotIn('<video', page)
                self.assertLess(page.index(loop), page.index('data-launch'))
        home = (root / 'index.html').read_text()
        self.assertIn('fetchpriority="high"', home)
        self.assertIn('data-motion-toggle', home)
        self.assertNotIn('<video', home)
        # Video and image media are served as-is: never gzip-compressed.
        self.assertFalse([p for p in (root / 'media').rglob('*.gz') if not p.name.endswith('.json.gz')])

    def test_current_workflows_are_in_navigation_and_search(self):
        root = ROOT / 'dist/site'
        gallery = (root / 'play/index.html').read_text()
        self.assertIn('Pick a game. Press play.', gallery)
        # Every card carries both filter axes; every genre filter matches at least one card.
        self.assertEqual(gallery.count('class="example-tile"'), len(json.loads((ROOT / 'examples/catalog.json').read_text())))
        for genre in re.findall(r'<button type="button" data-genre="([a-z]+)"', gallery):
            if genre != 'all':
                self.assertIn(f'data-genre="{genre}" data-search=', gallery)
        for slug in ('engine-workflows', 'responsive-games'):
            url = '/docs/' + slug + '/'
            for path in ('index.html', 'docs/index.html', 'docs/getting-started/index.html'):
                self.assertIn(url, (root / path).read_text())
            self.assertIn(url, (root / 'search.json').read_text())

    def test_content_build_rejects_retired_modules_and_api_cache(self):
        output = ROOT / 'dist/site/index.html'
        before = output.read_bytes()
        module = ROOT / 'dist/web/__retired_fixture__.wasm'
        api = ROOT / 'target/doc/__retired_fixture__'
        for fixture in (module, api):
            with self.subTest(path=fixture):
                if fixture == module:
                    with fixture.open('xb') as stream:
                        stream.write(b'old module')
                else:
                    fixture.mkdir()
                try:
                    result = subprocess.run([sys.executable, 'scripts/build-site.py', '--skip-build'], cwd=ROOT, capture_output=True, text=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('Stale build inputs', result.stderr)
                    self.assertEqual(output.read_bytes(), before)
                finally:
                    fixture.unlink() if fixture == module else fixture.rmdir()

if __name__ == '__main__':
    unittest.main()
