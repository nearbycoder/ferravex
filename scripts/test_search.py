#!/usr/bin/env python3
"""Check local corpus extraction, browser ranking and built-site compatibility.

The CorpusTests and BrowserSearchTests suites use isolated fixtures and need no
site build. SearchTests additionally checks the parent's existing dist/site.
"""
import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit
from site_search import ContentParser, browser_search_module, build_index, entry
ROOT=Path(__file__).resolve().parent.parent


def check_browser(test, rows, assertions):
    """Execute the actual generated ES module against the pinned local vendor."""
    script = r'''
import assert from 'node:assert/strict';
import fs from 'node:fs';
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const source = input.source.replace("'./vendor/qikbar/utils.js'", JSON.stringify(input.utils));
globalThis.fetch = () => { throw new Error('Search must not perform network I/O'); };
const {prepareIndex, searchIndex} = await import(
  'data:text/javascript;base64,' + Buffer.from(source).toString('base64'));
const corpus = input.rows;
const original = JSON.stringify(corpus);
const rows = prepareIndex(corpus);
const search = (query, options) => searchIndex(rows, query, options);
const hrefs = results => results.map(row => row.href);
''' + assertions + '''
assert.equal(JSON.stringify(corpus), original, 'Input rows must not change');
'''
    result = subprocess.run(
        ['node', '--input-type=module', '-e', script],
        input=json.dumps({
            'source': browser_search_module(), 'rows': rows,
            'utils': (ROOT / 'website/static/vendor/qikbar/utils.js').as_uri(),
        }), text=True, capture_output=True, timeout=30, cwd=ROOT)
    test.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class CorpusTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write('website/releases/player30.json', '[]')
        self.write('docs/camera/index.html', '''<article class="prose">
<h1>Camera guide</h1><p>Start here.</p><h2 id="timing">Timing</h2>
<p>Use <code>fixed_ticks</code> with Rig<wbr>3.<br>Keep interpolation.</p>
<script>must_not_leak</script><h2 id="collision">Collision</h2>
<p>Collision-only passage.</p></article>''')
        self.write('api/ferravex_play/camera/struct.Rig3.html', '''
<title>Rig3 in ferravex_play::camera - Rust</title>
<meta name="description" content="Generic metadata fallback.">
<nav>must_not_leak</nav><main id="main-content">
<div class="main-heading"><h1>Struct Rig<wbr>3</h1>
<a class="src" href="../../src/ferravex_play/camera.rs.html">Source</a></div>
<pre>pub struct Rig3</pre><details><summary class="hideme">Expand description</summary>
<div class="docblock"><p>Copyable camera presentation state.</p>
<p>Preserves <code>fixed_ticks</code> boundaries.</p></div></details>
<h2 id="implementations">Implementations</h2>
<details class="method-toggle"><summary><section id="method.update">
<a class="src" href="../../src/ferravex_play/camera.rs.html">Source</a>
<h4>pub fn <a>update</a>(&amp;mut self, dt: f32)</h4></section></summary>
<div class="docblock"><p>Advance presentation safely.</p>
<p>Reject invalid elapsed seconds.</p></div></details>
<section id="method.snap"><a class="src" href="../../src/ferravex_play/camera.rs.html">Source</a>
<h4>pub fn snap(&amp;mut self)</h4></section>
<section id="associatedconstant.MAX_STEP">
<a class="src" href="../../src/ferravex_play/camera.rs.html">Source</a>
<h4>pub const MAX_STEP: f32</h4></section>
<div id="blanket-implementations-list"><section id="method.borrow_mut">
<a class="src" href="../../src/ferravex_play/camera.rs.html">Source</a>
<h4>borrow_mut</h4></section></div></main>''')
        self.write('api/ferravex_play/camera/index.html', '''
<title>ferravex_play::camera - Rust</title><main id="main-content">
<div class="docblock"><p>Camera controls.</p></div></main>''')
        for name, description in (
                ('undocumented', 'API documentation for the Rust function.'),
                ('described', 'Configure the presentation camera.')):
            self.write(f'api/ferravex_play/camera/fn.{name}.html', f'''
<title>{name} in ferravex_play::camera - Rust</title>
<meta name="description" content="{description}"><main id="main-content">
<pre>pub fn {name}(ticks: u32)</pre></main>''')
        self.docs = [('camera', 'Camera guide', 'Engine', 'unused.md')]
        self.rows = build_index(self.root, self.docs, [], self.root)
        self.by_url = {row['url']: row for row in self.rows}

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def test_row_contract_and_local_unique_destinations(self):
        self.assertEqual(len(self.rows), len(self.by_url))
        for row in self.rows:
            self.assertEqual(set(row), {
                'id', 'title', 'url', 'section', 'subtitle', 'icon', 'text',
            })
            self.assertTrue(all(isinstance(value, str) for value in row.values()))
            self.assertEqual(row['id'], row['url'])
            self.assertTrue(row['url'].startswith('/'))
            self.assertNotIn('must_not_leak', row['text'])

    def test_api_prose_precedes_intact_signatures_and_qualified_paths(self):
        url = '/api/ferravex_play/camera/struct.Rig3.html'
        row = self.by_url[url]
        self.assertTrue(row['text'].startswith('Copyable camera presentation state.'))
        self.assertIn('Preserves fixed_ticks boundaries.', row['text'])
        self.assertIn('Struct Rig3', row['text'])
        self.assertIn('ferravex_play::camera::Rig3', row['text'])
        self.assertNotIn('Advance presentation', row['text'])
        for boilerplate in ('Source', 'Expand description', 'borrow_mut'):
            self.assertNotIn(boilerplate, row['text'])
        method = self.by_url[url + '#method.update']
        self.assertTrue(method['text'].startswith('Advance presentation safely.'))
        self.assertIn('Reject invalid elapsed seconds.', method['text'])
        self.assertIn('pub fn update(&mut self, dt: f32)', method['text'])
        self.assertIn('ferravex_play::camera::Rig3::update', method['text'])
        self.assertNotIn('Source', method['text'])
        self.assertNotIn('snap', method['text'])
        self.assertNotIn(url + '#method.borrow_mut', self.by_url)

    def test_undocumented_members_keep_signatures_and_modules_keep_paths(self):
        base = '/api/ferravex_play/camera/struct.Rig3.html#'
        self.assertTrue(self.by_url[base + 'method.snap']['text'].startswith('pub fn snap'))
        self.assertIn('MAX_STEP: f32', self.by_url[base + 'associatedconstant.MAX_STEP']['text'])
        module = self.by_url['/api/ferravex_play/camera/']
        self.assertEqual(module['text'], 'Camera controls. ferravex_play::camera')

    def test_guide_sections_keep_word_boundaries_without_neighbouring_text(self):
        row = self.by_url['/docs/camera/#timing']
        self.assertEqual(row['text'], 'Timing Use fixed_ticks with Rig3. Keep interpolation.')
        self.assertNotIn('Collision-only', row['text'])

    def test_metadata_fallback_uses_real_descriptions_or_the_signature(self):
        base = '/api/ferravex_play/camera/'
        undocumented = self.by_url[base + 'fn.undocumented.html']['text']
        self.assertTrue(undocumented.startswith('pub fn undocumented(ticks: u32)'))
        self.assertNotIn('API documentation', undocumented)
        described = self.by_url[base + 'fn.described.html']['text']
        self.assertTrue(described.startswith('Configure the presentation camera.'))

    def test_generated_rows_work_in_the_browser_without_a_schema_change(self):
        check_browser(self, self.rows, r'''
const target = '/api/ferravex_play/camera/struct.Rig3.html#method.update';
for (const query of ['update', 'upd', 'Rig3::update', 'camera::Rig3::upd',
                     'ferravex_play::camera::Rig3::update']) {
  assert.equal(search(query, {titleOnly: true})[0].href, target, query);
}
assert.match(search('Rig3::update')[0].subtitle, /Advance presentation safely/);
assert.doesNotMatch(search('Rig3::update')[0].subtitle, /Source|Expand description/);
assert.equal(search('ferravex_play::camera')[0].href, '/api/ferravex_play/camera/');
''')


class BrowserSearchTests(unittest.TestCase):
    def setUp(self):
        api = '/api/ferravex_play/camera/'
        self.rows = [
            entry('Rig3', api + 'struct.Rig3.html', 'API reference',
                  'Copyable presentation state.', 'ferravex_play::camera · struct', 'code'),
            entry('Rig30', api + 'struct.Rig30.html', 'API reference'),
            entry('Rig3::update', api + 'struct.Rig3.html#method.update', 'API reference',
                  'Advance presentation safely. Reject invalid elapsed seconds.',
                  'ferravex_play::camera', 'code'),
            entry('Rig3::update_all', api + 'struct.Rig3.html#method.update_all', 'API reference'),
            entry('Rig3::r#type', api + 'struct.Rig3.html#method.r%23type', 'API reference'),
            entry('Rig3::MAX_STEP', api + 'struct.Rig3.html#associatedconstant.MAX_STEP', 'API reference'),
            entry('cast_ball', api + 'fn.cast_ball.html', 'API reference'),
            entry('cast_balloon', api + 'fn.cast_balloon.html', 'API reference'),
            entry('castball', api + 'fn.castball.html', 'API reference'),
            entry('game', '/api/ferravex_render/macro.game.html', 'API reference'),
            entry('Élan', api + 'struct.%C3%89lan.html', 'API reference'),
            entry('Camera guide', '/docs/camera/', 'Documentation',
                  'Use ferravex_play::camera::Rig3::update for the camera.', 'Guides'),
            entry('ferravex_play::camera::Rig3::update', '/docs/update/', 'Documentation'),
            entry('Marble run', '/examples/marble-run/', 'Examples', 'Rolling marble game.'),
        ]

    def check(self, assertions, rows=None):
        check_browser(self, self.rows if rows is None else rows, assertions)

    def test_exact_symbols_precede_prefixes_fuzzy_names_and_prose(self):
        self.check(r'''
const api = '/api/ferravex_play/camera/';
assert.equal(search('Rig3')[0].href, api + 'struct.Rig3.html');
assert.equal(search('Rig')[0].href, api + 'struct.Rig3.html');
assert.equal(search('cast_ball')[0].href, api + 'fn.cast_ball.html');
assert.equal(search('castball')[0].href, api + 'fn.castball.html');
const update = search('update');
assert.equal(update[0].href, api + 'struct.Rig3.html#method.update');
assert.equal(update[1].href, api + 'struct.Rig3.html#method.update_all');
assert.equal(search('Rig3::upd')[0].href, api + 'struct.Rig3.html#method.update');
''')

    def test_qualified_paths_work_in_best_matches_without_prose_leakage(self):
        self.check(r'''
const target = '/api/ferravex_play/camera/struct.Rig3.html#method.update';
for (const query of ['ferravex_play::camera::Rig3::update', 'camera::Rig3::upd',
                     'Rig3 :: upd', '::ferravex_play::camera::Rig3::update()',
                     '`Rig3::update`', 'RIG3::UPDATE']) {
  assert.equal(search(query)[0].href, target, query);
  assert.equal(search(query, {titleOnly: true})[0].href, target, query);
}
assert.equal(search('ferravex_play::camera::Rig3::update',
  {section: 'Documentation'})[0].href, '/docs/update/');
assert.ok(search('ig3::update').length, 'Fuzzy fallback remains available');
''')

    def test_raw_identifiers_macros_constants_modules_and_unicode(self):
        rows = self.rows + [
            entry('ferravex_play::camera', '/api/ferravex_play/camera/', 'API reference'),
            entry('ferravex_play', '/api/ferravex_play/index.html', 'API reference'),
            entry('Camera::tick', '/api/ferravex_play/camera/trait.Camera.html#tymethod.tick',
                  'API reference'),
            entry('Camera::Frame', '/api/ferravex_play/camera/trait.Camera.html#associatedtype.Frame',
                  'API reference'),
            entry('Camera::r#match', '/api/ferravex_play/camera/trait.Camera.html#tymethod.r#match',
                  'API reference'),
        ]
        self.check(r'''
const api = '/api/ferravex_play/camera/';
assert.equal(search('r#type')[0].href, api + 'struct.Rig3.html#method.r%23type');
assert.equal(search('Rig3::r#type')[0].href, api + 'struct.Rig3.html#method.r%23type');
assert.equal(search('game!')[0].href, '/api/ferravex_render/macro.game.html');
assert.equal(search('MAX_STEP')[0].href, api + 'struct.Rig3.html#associatedconstant.MAX_STEP');
assert.equal(search('élan')[0].href, api + 'struct.%C3%89lan.html');
assert.equal(search('camera')[0].href, api);
assert.equal(search('ferravex_play')[0].href, '/api/ferravex_play/index.html');
assert.ok(search('ferravex_play::camera::', {titleOnly: true}).length);
assert.equal(search('camera::Camera::tick')[0].href, api + 'trait.Camera.html#tymethod.tick');
assert.equal(search('camera::Camera::Frame')[0].href, api + 'trait.Camera.html#associatedtype.Frame');
assert.equal(search('Camera::r#match')[0].href, api + 'trait.Camera.html#tymethod.r#match');
''', rows)

    def test_qualified_paths_disambiguate_equal_names_and_module_components(self):
        rows = self.rows + [
            entry('Rig3::update', '/api/ferravex_other/camera/struct.Rig3.html#method.update',
                  'API reference'),
            entry('Rig3::update', '/api/ferravex_play/othercamera/struct.Rig3.html#method.update',
                  'API reference'),
        ]
        self.check(r'''
assert.equal(search('ferravex_other::camera::Rig3::update', {titleOnly:true})[0].href,
             '/api/ferravex_other/camera/struct.Rig3.html#method.update');
const matches = hrefs(search('camera::Rig3::update', {titleOnly:true}));
assert.ok(matches.includes('/api/ferravex_play/camera/struct.Rig3.html#method.update'));
assert.ok(!matches.includes('/api/ferravex_play/othercamera/struct.Rig3.html#method.update'));
const fullText = hrefs(search('camera::Rig3::update'));
assert.ok(fullText.indexOf('/api/ferravex_play/camera/struct.Rig3.html#method.update')
  < fullText.indexOf('/api/ferravex_play/othercamera/struct.Rig3.html#method.update'));
''', rows)

    def test_ties_are_independent_of_input_order_and_locale(self):
        rows = [
            entry('Same', '/api/ferravex_b/struct.Same.html', 'API reference'),
            entry('Same', '/api/ferravex_a/type.Same.html', 'API reference'),
            entry('Same', '/api/ferravex_a/struct.Same.html', 'API reference'),
            entry('Same guide', '/docs/z/', 'Documentation'),
            entry('Same guide', '/docs/a/', 'Documentation'),
        ]
        self.check(r'''
String.prototype.localeCompare = () => { throw new Error('Locale-dependent tie'); };
const expected = ['/api/ferravex_a/struct.Same.html', '/api/ferravex_a/type.Same.html',
                  '/api/ferravex_b/struct.Same.html', '/docs/a/', '/docs/z/'];
for (let turn = 0; turn < corpus.length; turn++) {
  const shuffled = corpus.slice(turn).concat(corpus.slice(0, turn)).reverse();
  assert.deepEqual(hrefs(searchIndex(prepareIndex(shuffled), 'Same')), expected);
}
''', rows)

    def test_named_snippets_explain_api_behaviour_and_keep_context(self):
        self.check(r'''
const result = search('Rig3::update')[0];
assert.match(result.subtitle, /^ferravex_play::camera — Advance presentation safely/);
assert.match(result.subtitle, /Reject invalid elapsed seconds/);
assert.deepEqual(Object.keys(result).sort(), ['accessories', 'href', 'icon', 'id', 'subtitle', 'title']);
assert.deepEqual(result.accessories, [{text: 'Rust API'}]);
assert.equal(search('marble')[0].href, '/examples/marble-run/');
''')

    def test_prose_snippets_prefer_a_passage_covering_all_query_terms(self):
        rows = [entry('Timing guide', '/docs/timing/', 'Documentation',
                      'The engine starts here. ' + 'Background material. ' * 30
                      + 'The camera keeps café movement smooth with fixed_ticks and interpolation. '
                      + 'Closing material. ' * 20, 'Engine')]
        self.check(r'''
const result = search('the café interpolation')[0];
assert.ok(result);
assert.match(result.subtitle, /café movement smooth/);
assert.match(result.subtitle, /interpolation/);
assert.match(result.subtitle, /Engine — …/);
assert.ok(result.subtitle.length <= 215);
assert.ok(result.subtitle.endsWith('…'));
''', rows)

    def test_empty_missing_malformed_inputs_and_bounds(self):
        self.check(r'''
for (const query of ['', '   ', null, 1, '<script>alert(1)</script>', 'unfindablequokkarobot']) {
  assert.deepEqual(search(query), []);
}
for (const limit of [-1, 0, 0.5, NaN, Infinity, '8']) {
  assert.deepEqual(search('Rig', {limit}), []);
}
assert.equal(search('Rig', {limit: 1}).length, 1);
assert.deepEqual(prepareIndex(null), []);
assert.deepEqual(prepareIndex([null, {}, {title:'x',url:'https://example.com/'},
  {title:'x',url:'//example.com/'}, {title:'x',url:'/\\example.com/'},
  {title:'x',url:'/\n/example.com/'}]), []);
const valid = prepareIndex([{title:'Malformed', url:'/api/%broken/struct.Foo.html',
                            section:'API reference', text:42, subtitle:null}]);
assert.equal(searchIndex(valid, 'Malformed').length, 1);
assert.equal(searchIndex(valid, 'Malformed')[0].subtitle, '');
const many = prepareIndex(Array.from({length:200}, (_, n) => ({
  title:'Match', url:`/docs/${n}/`, text:'Match'})));
assert.equal(searchIndex(many, 'match', {limit:10000}).length, 100);
const longQuery = prepareIndex([{title:'x'.repeat(160), url:'/long-query/'}]);
assert.equal(searchIndex(longQuery, 'x'.repeat(200))[0].href, '/long-query/');
''')


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
        self.assertIn('/docs/cameras/',urls)
        self.assertIn('/api/ferravex_play/camera/struct.Rig3.html#method.update',urls)
        self.assertIn('/api/ferravex_engine/camera/struct.RigidObstructions.html',urls)
        self.assertFalse(any('/camera/follow/' in u for u in urls),'private-module Rustdoc redirects must not be indexed')
        self.assertFalse(any('/api/ferravex_engine/camera/struct.Rig3.html' in u or '/api/ferravex_engine/camera/trait.CollisionQuery.html' in u for u in urls),'camera re-export mirrors must use the defining gameplay crate')
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

    def test_api_parser_keeps_methods_with_and_without_doc_wrappers(self):
        parser=ContentParser(api=True)
        parser.feed('<main id="main-content"><div class="main-heading"><a class="src" href="../../src/ferravex_play/camera/mod.rs.html">Source</a></div>'
                    '<h2 id="implementations">Implementations</h2>'
                    '<section id="method.update"><a class="src" href="../../src/ferravex_play/camera/mod.rs.html">Source</a><h4>pub fn update</h4></section>'
                    '<details class="method-toggle"><summary><section id="method.cut"><a class="src" href="../../src/ferravex_play/camera/mod.rs.html">Source</a><h4>pub fn cut</h4></section></summary><p>Teleport feedback.</p></details></main>')
        self.assertEqual(parser.definition_crate,'ferravex_play')
        self.assertEqual([m['anchor'] for m in parser.methods],['method.update','method.cut'])
        self.assertIn('Teleport feedback',''.join(parser.methods[1]['text']))
        redirect=ContentParser(api=True);redirect.feed('<title>Redirection</title><meta http-equiv="refresh" content="0;URL=../index.html">')
        self.assertFalse(redirect.has_main)
        mirror=ContentParser(api=True);mirror.feed('<main id="main-content"><div class="main-heading">A re-exported trait</div><section id="implementors-list"><a class="src" href="../../src/ferravex_engine/camera.rs.html">Impl source</a></section></main>')
        self.assertIsNone(mirror.definition_crate,'an implementation source must not masquerade as the defining trait source')

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

    def test_generated_browser_scorer_retains_existing_corpus_queries(self):
        check_browser(self, self.rows, r'''
const expected = [
  ['salvage pinball', '/examples/salvage-pinball/'],
  ['marble', '/examples/marble-run/'],
  ['dune skim', '/examples/dune-skimmer/'],
  ['coyote', '/examples/emberline/', {section: 'Examples'}],
  ['castball', '/api/ferravex_dynamics/struct.PhysicsWorld.html#method.cast_ball'],
  ['PhysicsWorld::cast_ball', '/api/ferravex_dynamics/struct.PhysicsWorld.html#method.cast_ball'],
  ['PlayerInput::sample', '/api/ferravex_render/input/struct.PlayerInput.html#method.sample'],
  ['ActionMap::named_action', '/api/ferravex_play/input/struct.ActionMap.html#method.named_action'],
  ['Rig3::update', '/api/ferravex_play/camera/struct.Rig3.html#method.update'],
  ['RigidObstructions', '/api/ferravex_engine/camera/struct.RigidObstructions.html'],
  ['ferravex_play::camera::Rig3::update', '/api/ferravex_play/camera/struct.Rig3.html#method.update'],
];
for (const [query, href, options] of expected) {
  assert.equal(search(query, options)[0].href, href, query);
}
assert.ok(search('Netcode', {section:'Documentation'})[0].href.includes('#netcode'));
assert.ok(search('workspace optional platform', {section:'Documentation'})
  .some(row => row.href.startsWith('/docs/dependencies/')));
''')

class AnchorParser(ContentParser):
    def __init__(self):super().__init__();self.ids=set()
    def handle_starttag(self,tag,attrs):
        value=dict(attrs).get('id')
        if value:self.ids.add(value)

if __name__=='__main__':unittest.main()
