#!/usr/bin/env python3
"""Check site-wide search destinations, corpus coverage and the pinned Qikbar bundle."""
import hashlib
import json
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit
from site_search import ContentParser
ROOT=Path(__file__).resolve().parent.parent

class SearchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.site=ROOT/'dist/site'
        cls.rows=json.loads((cls.site/'search.json').read_text())

    def test_current_catalog_and_sections_are_searchable(self):
        urls={row['url'] for row in self.rows}
        for game in json.loads((ROOT/'examples/catalog.json').read_text()):
            self.assertIn('/examples/'+game['id']+'/',urls)
        self.assertIn('/docs/architecture/#netcode',urls)
        self.assertIn('/api/ferravex_dynamics/struct.PhysicsWorld.html#method.cast_ball',urls)
        self.assertIn('/api/ferravex_net/struct.Snapshot.html',urls)
        self.assertFalse(any('/api/ferravex/dynamics/' in url for url in urls),'facade mirrors must not duplicate defining-crate results')

    def test_all_search_destinations_and_anchors_exist(self):
        ids={}
        for row in self.rows:
            url=urlsplit(row['url'])
            self.assertFalse(url.scheme or url.netloc)
            path=self.site/unquote(url.path.lstrip('/'))
            if path.is_dir():path=path/'index.html'
            self.assertTrue(path.is_file(),row['url'])
            if url.fragment:
                if path not in ids:
                    parser=AnchorParser();parser.feed(path.read_text());ids[path]=parser.ids
                self.assertIn(url.fragment,ids[path],row['url'])
        self.assertEqual(len(self.rows),len({row['url'] for row in self.rows}))

    def test_docs_content_and_api_method_body_survive_indexing(self):
        rows={row['url']:row for row in self.rows}
        self.assertIn('coyote',rows['/examples/emberline/']['text'].lower())
        method=rows['/api/ferravex_dynamics/struct.PhysicsWorld.html#method.cast_ball']
        self.assertIn('cast_ball',method['text'])
        self.assertNotIn('borrow_mut',method['text'])

    def test_pinned_modules_are_complete_and_unchanged(self):
        base=ROOT/'website/static/vendor/qikbar'
        manifest=json.loads((base/'manifest.json').read_text())
        self.assertEqual(manifest['version'],'0.2.1')
        for file,digest in manifest['files'].items():
            self.assertEqual(hashlib.sha256((base/file).read_bytes()).hexdigest(),digest,file)
        self.assertIn('MIT License',(base/'LICENSE').read_text())
        self.assertTrue((self.site/'vendor/qikbar/index.js').is_file())
        self.assertTrue(list((self.site/'vendor/qikbar/chunks').glob('bar-*.js')))
        api=(self.site/'api/ferravex/index.html').read_text()
        self.assertIn('/site-search.js',api)
        self.assertIn('data-ferravex-api',api)

class AnchorParser(ContentParser):
    def __init__(self):super().__init__();self.ids=set()
    def handle_starttag(self,tag,attrs):
        value=dict(attrs).get('id')
        if value:self.ids.add(value)

if __name__=='__main__':unittest.main()
