import assert from 'node:assert/strict';
import {readFileSync, statSync} from 'node:fs';
import test from 'node:test';
import {filterGames, initCatalog} from '../catalog.js';

const pageUrl = new URL('../index.html', import.meta.url);
const html = readFileSync(pageUrl, 'utf8');
const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
const titles = games => games.map(game => game.title);
const cards = [...html.matchAll(/<a\b([^>]*\bdata-game\b[^>]*)>([\s\S]*?)<\/a>/g)].map(([, attributes, content]) => ({
  attributes,
  href: attributes.match(/\bhref="([^"]+)"/)[1],
  dimension: attributes.match(/\bdata-dimension="([^"]+)"/)[1],
  title: content.match(/<strong>([^<]+)<\/strong>/)[1],
  genre: content.match(/<span class="genre">([^<]+)<\/span>/)[1],
  description: content.match(/<small>([^<]+)<\/small>/)[1],
}));

test('all 15 original game links and displayed metadata match real local hosts', () => {
  assert.equal(cards.length, 15);
  assert.equal(new Set(cards.map(card => card.href)).size, 15);
  assert.deepEqual(cards.map(card => card.href), catalog.map(game => `${game.host}/`));
  for (const [index, card] of cards.entries()) {
    assert.match(card.href, /^[a-z0-9-]+\/$/);
    assert.ok(statSync(new URL(`${card.href}index.html`, pageUrl)).isFile(), card.href);
    assert.equal(card.title, catalog[index].title);
    assert.equal(card.dimension, catalog[index].dimension);
    assert.equal(card.genre, `${catalog[index].dimension} · ${catalog[index].genre}`);
    assert.equal(card.description, catalog[index].description);
    assert.doesNotMatch(card.attributes, /\bhidden\b|\btabindex=/, 'links remain available without JavaScript');
  }
  assert.match(html, /<script type="module" src="catalog.js"><\/script>/);
  assert.ok(statSync(new URL('../catalog.js', import.meta.url)).isFile());
});

test('search combines words across name, genre, description, and dimension', () => {
  assert.deepEqual(titles(filterGames(cards, {query: '  PHYSICS    lab  ', dimension: '3D'})), ['3D Physics Lab']);
  assert.deepEqual(titles(filterGames(cards, {query: 'tower defense orchard'})), ['Orchard Guard']);
  assert.deepEqual(titles(filterGames(cards, {query: '3d low-poly boost'})), ['Dune Skimmer']);
  assert.deepEqual(titles(filterGames(cards, {query: 'crýstal   gate'})), ['Crystal Cavern']);
  assert.deepEqual(titles(filterGames(cards, {query: 'FPS'})), ['Courtyard', 'Strike']);
  assert.deepEqual(filterGames(cards, {query: 'physics orchard'}), []);
  assert.deepEqual(filterGames(cards, {query: '<script>alert(1)</script>'}), []);
});

test('empty searches and dimension filters preserve ordering and input records', () => {
  const games = Object.freeze(cards.map(card => Object.freeze({...card})));
  assert.deepEqual(filterGames(games), games);
  assert.deepEqual(filterGames(games, {query: '  — /  '}), games);
  assert.notEqual(filterGames(games), games);
  assert.equal(filterGames(games, {dimension: '2D'}).length, 7);
  assert.equal(filterGames(games, {dimension: '3D'}).length, 8);
  assert.deepEqual(filterGames(games, {dimension: '4D'}), []);
  assert.deepEqual(filterGames([], {query: 'physics'}), []);
  assert.equal(filterGames(games, {query: 'ember'})[0], games[2]);
});

test('static controls have labels, live result text, and a skip destination', () => {
  assert.match(html, /<label[^>]*for="catalog-search"/);
  assert.match(html, /<input[^>]*id="catalog-search"[^>]*type="search"[^>]*aria-describedby="search-help"/);
  assert.match(html, /<fieldset[^>]*aria-describedby="dimension-help"[^>]*>[\s\S]*?<legend>Dimensions<\/legend>/);
  assert.deepEqual([...html.matchAll(/type="radio" name="dimension" value="([^"]+)"/g)].map(match => match[1]), ['all', '2D', '3D']);
  assert.match(html, /id="result-count" role="status" aria-live="polite" aria-atomic="true"/);
  assert.match(html, /href="#games-heading"/);
  assert.match(html, /id="games-heading" tabindex="-1"/);
  assert.match(html, /id="catalog-filters"[^>]*\bhidden/);
  assert.match(html, /\[hidden\]\s*\{\s*display:\s*none\s*!important/);
  assert.match(html, /<input[^>]*id="catalog-search"[^>]*maxlength="200"[^>]*enterkeyhint="search"/);
  assert.match(html, /<button type="submit" aria-controls="games">View results<\/button>/);
  assert.match(html, /id="empty-heading"[^>]*aria-describedby="empty-description"/);
  assert.match(html, /id="empty-description"/);
  assert.match(html, /<noscript>[\s\S]*All fifteen examples are listed below/);
});

// Minimal first-party event/focus doubles exercise the real controller without
// a DOM package. They do not claim to verify native browser layout/tab behavior.
function fixture({query = '', dimension = 'all', url, historyState = null, historyError = false, prepare = () => {}} = {}) {
  const document = {activeElement: null, defaultView: new EventTarget()};
  const view = document.defaultView;
  const replacements = [];
  if (url) {
    view.location = new URL(url);
    view.history = {
      state: historyState,
      replaceState(state, title, href) {
        if (historyError) throw new DOMException('History access is restricted', 'SecurityError');
        replacements.push({state, title, href});
        this.state = state;
        view.location = new URL(href);
      },
    };
  }
  class Element extends EventTarget {
    hidden = false;
    value = '';
    checked = false;
    textWrites = 0;
    #textContent = '';
    get textContent() { return this.#textContent; }
    set textContent(value) { this.#textContent = value; this.textWrites++; }
    children = new Map();
    ownerDocument = document;
    querySelector(selector) { return this.children.get(selector) ?? null; }
    contains(node) { return node === this || [...this.children.values()].some(child => child.contains(node)); }
    focus() { document.activeElement = this; }
  }
  const container = new Element();
  const root = {querySelector: selector => selector === '[data-catalog]' ? container : null};
  const controls = {};
  for (const id of ['catalog-filters', 'catalog-search', 'result-count', 'catalog-empty', 'empty-heading', 'empty-description', 'clear-filters', 'show-all', 'search-all-dimensions']) {
    controls[id] = new Element();
    container.children.set(`#${id}`, controls[id]);
  }
  controls['catalog-empty'].children.set('h3', controls['empty-heading']);
  controls['catalog-empty'].children.set('button', controls['show-all']);
  controls['catalog-empty'].children.set('p', controls['empty-description']);
  controls['catalog-empty'].children.set('#search-all-dimensions', controls['search-all-dimensions']);
  controls['catalog-filters'].hidden = true;
  controls['catalog-search'].value = query;
  const dimensions = ['all', '2D', '3D'].map(value => Object.assign(new Element(), {value, checked: value === dimension}));
  const links = cards.map(card => {
    const link = new Element();
    link.dataset = {dimension: card.dimension};
    for (const [selector, value] of [['strong', card.title], ['small', card.description], ['.genre', card.genre]]) {
      link.children.set(selector, Object.assign(new Element(), {textContent: value}));
    }
    return link;
  });
  container.querySelectorAll = selector => selector === '[data-game]' ? links : dimensions;
  prepare({document, container, controls, dimensions, links});
  initCatalog(root);
  function send(element, type, properties = {}) {
    const event = Object.assign(new Event(type, {cancelable: true}), properties);
    element.dispatchEvent(event);
    return event;
  }
  function search(query) {
    controls['catalog-search'].value = query;
    send(controls['catalog-search'], 'input');
  }
  function select(dimension) {
    for (const input of dimensions) input.checked = input.value === dimension;
    send(dimensions.find(input => input.checked), 'change');
  }
  return {document, controls, dimensions, links, send, search, select, replacements, view, visible: () => links.filter(link => !link.hidden)};
}

test('controller filters immediately, updates results, and keeps typing focus', () => {
  const page = fixture();
  assert.equal(page.controls['catalog-filters'].hidden, false);
  assert.equal(page.visible().length, 15);
  page.controls['catalog-search'].focus();
  page.search('physics lab');
  assert.equal(page.visible().length, 2);
  page.select('3D');
  assert.equal(page.visible().length, 1);
  assert.equal(page.controls['result-count'].textContent, 'Showing 1 of 15 examples matching “physics lab” in 3D.');
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  const submit = page.send(page.controls['catalog-filters'], 'submit');
  assert.equal(submit.defaultPrevented, true, 'Enter must not navigate/reload');
  assert.equal(page.document.activeElement, page.visible()[0], 'Enter focuses the first visible result');
});

test('Escape clears only the query and respects composition', () => {
  const page = fixture({query: 'physics', dimension: '3D'});
  const search = page.controls['catalog-search'];
  search.focus();
  page.send(search, 'keydown', {key: 'Escape', isComposing: true});
  assert.equal(search.value, 'physics');
  const escape = page.send(search, 'keydown', {key: 'Escape'});
  assert.equal(escape.defaultPrevented, true);
  assert.equal(search.value, '');
  assert.equal(page.visible().length, 8, 'dimension stays selected');
  assert.equal(page.document.activeElement, search);
  assert.equal(page.send(search, 'keydown', {key: 'Escape'}).defaultPrevented, false);
});

test('no matches has a focus destination and recovery restores all links', () => {
  const page = fixture({query: 'no such example', dimension: '2D'});
  assert.equal(page.visible().length, 0);
  assert.equal(page.controls['catalog-empty'].hidden, false);
  assert.match(page.controls['result-count'].textContent, /No matching examples/);
  page.send(page.controls['catalog-filters'], 'submit');
  assert.equal(page.document.activeElement, page.controls['empty-heading']);
  page.controls['show-all'].focus();
  page.send(page.controls['show-all'], 'click');
  assert.equal(page.visible().length, 15);
  assert.equal(page.controls['catalog-empty'].hidden, true);
  assert.equal(page.controls['catalog-search'].value, '');
  assert.equal(page.dimensions[0].checked, true);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  page.select('3D');
  page.search('physics');
  page.send(page.controls['clear-filters'], 'click');
  assert.equal(page.visible().length, 15);
});

test('history-restored filters refresh and never leave focus inside hidden content', () => {
  const page = fixture({query: 'ember'});
  assert.equal(page.visible().length, 1);
  page.visible()[0].focus();
  page.controls['catalog-search'].value = 'nothing here';
  page.send(page.document.defaultView, 'pageshow');
  assert.equal(page.visible().length, 0);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  page.send(page.controls['catalog-filters'], 'submit');
  assert.equal(page.document.activeElement, page.controls['empty-heading']);
  page.controls['catalog-search'].value = '';
  page.send(page.document.defaultView, 'pageshow');
  assert.equal(page.visible().length, 15);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
});

test('optional text metadata never becomes a false search term or coerces objects', () => {
  const unexpected = {toString() { throw new Error('Metadata must not be coerced'); }};
  const games = [
    {title: 'Moon Garden', dimension: '2D'},
    {title: 'Night Garden', dimension: '3D', description: null, genre: unexpected},
    {title: 'Star Garden', dimension: '3D', description: 12, genre: false, capabilities: unexpected},
  ];
  assert.deepEqual(titles(filterGames(games, {query: 'garden'})), ['Moon Garden', 'Night Garden', 'Star Garden']);
  assert.deepEqual(titles(filterGames(games, {query: 'night garden', dimension: '3D'})), ['Night Garden']);
  for (const query of ['undefined', 'null', 'object', '12', 'false', 'capabilities']) {
    assert.deepEqual(filterGames(games, {query}), [], query);
  }
  assert.deepEqual(filterGames(games, {query: unexpected}), games);
  assert.deepEqual(filterGames(games, {query: null}), games);
});

test('optional capability metadata is ignored, including unknown schemas and malformed claims', () => {
  for (const capabilities of [undefined, null, false, [], 'keyboard', {schema_version: 999}, {schema_version: 1, controls: null}]) {
    const games = catalog.map(game => ({...game, capabilities}));
    assert.deepEqual(titles(filterGames(games, {query: 'physics lab', dimension: '3D'})), ['3D Physics Lab']);
    assert.deepEqual(filterGames(games, {query: 'runtime_verified'}), []);
  }
});

test('partial card metadata leaves links available and searchable by remaining fields', () => {
  const page = fixture({prepare({links}) {
    links[0].children.delete('small');
    links[0].children.delete('.genre');
    links[1].children.delete('strong');
    links[1].textContent = 'Orbit Breaker';
    delete links[2].dataset.dimension;
  }});
  assert.equal(page.visible().length, 15);
  page.search('Courtyard');
  assert.deepEqual(page.visible(), [page.links[0]]);
  page.search('Orbit');
  assert.deepEqual(page.visible(), [page.links[1]]);
  page.search('Emberline');
  assert.deepEqual(page.visible(), [page.links[2]]);
  page.search('undefined');
  assert.equal(page.visible().length, 0);
});

test('missing required controls retain the static catalog without partial enhancement', () => {
  assert.doesNotThrow(() => initCatalog({querySelector: () => null}));
  const page = fixture({prepare({container}) { container.children.delete('#result-count'); }});
  assert.equal(page.controls['catalog-filters'].hidden, true);
  assert.equal(page.visible().length, 15);
});

test('URL filters restore without rewriting the URL or stealing focus on initial load', () => {
  const url = 'https://example.test/play/?q=PHYSICS+lab&dimension=3D#games-heading';
  const page = fixture({url, query: 'stale restored form text', dimension: '2D'});
  assert.equal(page.controls['catalog-search'].value, 'PHYSICS lab');
  assert.equal(page.dimensions[2].checked, true);
  assert.deepEqual(page.visible(), [page.links[14]]);
  assert.equal(page.document.activeElement, null);
  assert.equal(page.view.location.href, url);
  assert.equal(page.replacements.length, 0);
});

test('filter edits round-trip Unicode/reserved characters and retain other URL state', () => {
  const historyState = {scroll: [10, 20], otherOwner: {id: 3}};
  const page = fixture({url: 'https://example.test/nested/play/index.html?ref=menu&tag=a&tag=b#games-heading', historyState});
  const query = 'crýstal & + / ? # 雪 🚀';
  page.search(query);
  page.select('3D');
  assert.equal(page.view.location.searchParams.get('q'), query);
  assert.equal(page.view.location.searchParams.get('dimension'), '3D');
  assert.equal(page.view.location.searchParams.get('ref'), 'menu');
  assert.deepEqual(page.view.location.searchParams.getAll('tag'), ['a', 'b']);
  assert.equal(page.view.location.hash, '#games-heading');
  assert.equal(page.view.location.pathname, '/nested/play/index.html');
  assert.equal(page.view.location.origin, 'https://example.test');
  assert.equal(page.view.history.state, historyState);
  assert.equal(page.replacements.length, 2);
  const shared = fixture({url: page.view.location.href});
  assert.equal(shared.controls['catalog-search'].value, query);
  assert.equal(shared.dimensions[2].checked, true);
  assert.equal(shared.visible().length, page.visible().length);
  page.search(query);
  page.select('3D');
  assert.equal(page.replacements.length, 2, 'unchanged filters do not rewrite history');
  page.send(page.controls['clear-filters'], 'click');
  assert.equal(page.view.location.searchParams.has('q'), false);
  assert.equal(page.view.location.searchParams.has('dimension'), false);
  assert.equal(page.view.location.searchParams.get('ref'), 'menu');
  assert.equal(page.view.location.hash, '#games-heading');
  assert.equal(page.visible().length, 15);
});

test('URL decoding tolerates empty, invalid and duplicate filter parameters', () => {
  for (const dimension of ['4D', '3d', '', 'null', 'all']) {
    const page = fixture({url: `https://example.test/?dimension=${dimension}`});
    assert.equal(page.dimensions[0].checked, true);
    assert.equal(page.visible().length, 15);
  }
  const page = fixture({url: 'https://example.test/?q=ember&q=physics&dimension=2D&dimension=3D&ref=keep'});
  assert.equal(page.controls['catalog-search'].value, 'ember');
  assert.equal(page.dimensions[1].checked, true);
  assert.deepEqual(page.visible(), [page.links[2]]);
  page.search('physics');
  assert.deepEqual(page.view.location.searchParams.getAll('q'), ['physics']);
  assert.deepEqual(page.view.location.searchParams.getAll('dimension'), ['2D']);
  assert.equal(page.view.location.searchParams.get('ref'), 'keep');
  assert.doesNotThrow(() => fixture({url: 'https://example.test/?q=%E0%A4%A&dimension=%ZZ'}));
  const empty = fixture({url: 'https://example.test/?q=&dimension=', query: 'stale', dimension: '3D'});
  assert.equal(empty.visible().length, 15);
  assert.equal(empty.controls['catalog-search'].value, '');
});

test('long shared/programmatic queries are bounded without splitting a surrogate pair', () => {
  const query = `${'x'.repeat(199)}🚀tail`;
  const page = fixture({url: `https://example.test/?q=${encodeURIComponent(query)}`});
  assert.equal(page.controls['catalog-search'].value, 'x'.repeat(199));
  assert.equal(page.controls['result-count'].textContent.includes('�'), false);
  page.search('a'.repeat(300));
  assert.equal(page.controls['catalog-search'].value.length, 200);
  assert.equal(page.view.location.searchParams.get('q').length, 200);
  page.search(`${'x'.repeat(198)}🚀tail`);
  assert.equal(page.controls['catalog-search'].value, `${'x'.repeat(198)}🚀`);
  assert.equal(fixture({url: page.view.location.href}).controls['catalog-search'].value, `${'x'.repeat(198)}🚀`);
});

test('Back/Forward and page restoration use URL state and rescue only hidden focus', () => {
  const page = fixture({url: 'https://example.test/play/?q=ember'});
  page.links[2].focus();
  page.view.location = new URL('https://example.test/play/?q=physics&dimension=3D');
  page.send(page.view, 'popstate');
  assert.equal(page.controls['catalog-search'].value, 'physics');
  assert.equal(page.dimensions[2].checked, true);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  assert.equal(page.visible().length, 3);
  page.links[14].focus();
  page.controls['catalog-search'].value = 'stale autofill';
  page.send(page.view, 'pageshow', {persisted: true});
  assert.equal(page.controls['catalog-search'].value, 'physics');
  assert.equal(page.document.activeElement, page.links[14], 'visible focused links keep focus');
  page.view.location = new URL('https://example.test/play/?q=no-such-game');
  page.send(page.view, 'popstate');
  page.send(page.controls['catalog-filters'], 'submit');
  assert.equal(page.document.activeElement, page.controls['empty-heading']);
  page.view.location = new URL('https://example.test/play/?ref=home');
  page.send(page.view, 'popstate');
  assert.equal(page.controls['catalog-search'].value, '');
  assert.equal(page.dimensions[0].checked, true);
  assert.equal(page.visible().length, 15);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  assert.equal(page.replacements.length, 0, 'history restoration does not overwrite navigated entries');
});

test('denied or unavailable history access does not break local filtering and recovery', () => {
  for (const options of [
    {url: 'https://example.test/', historyError: true},
    {url: 'https://example.test/', prepare({document}) { delete document.defaultView.history; }},
    {prepare({document}) { Object.defineProperty(document.defaultView, 'location', {get() { throw new Error('Unavailable'); }}); }},
    {prepare({document}) { document.defaultView = null; }},
  ]) {
    const page = fixture(options);
    page.search('physics lab');
    assert.equal(page.visible().length, 2);
    page.select('3D');
    assert.deepEqual(page.visible(), [page.links[14]]);
    page.send(page.controls['catalog-filters'], 'submit');
    assert.equal(page.document.activeElement, page.links[14]);
    page.search('no such game');
    page.send(page.controls['show-all'], 'click');
    assert.equal(page.visible().length, 15);
    assert.equal(page.document.activeElement, page.controls['catalog-search']);
  }
});

test('empty search can broaden the dimension while retaining the query', () => {
  const page = fixture({url: 'https://example.test/?q=ember&dimension=3D'});
  const broaden = page.controls['search-all-dimensions'];
  assert.equal(page.visible().length, 0);
  assert.equal(broaden.hidden, false);
  assert.equal(broaden.textContent, 'Show 1 example in all dimensions');
  assert.match(page.controls['empty-description'].textContent, /matches 1 example in other dimensions/);
  broaden.focus();
  page.send(broaden, 'click');
  assert.deepEqual(page.visible(), [page.links[2]]);
  assert.equal(page.controls['catalog-search'].value, 'ember');
  assert.equal(page.controls['catalog-empty'].hidden, true);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  assert.equal(page.view.location.searchParams.get('q'), 'ember');
  assert.equal(page.view.location.searchParams.has('dimension'), false);
});

test('no-results text identifies filters and hides irrelevant dimension recovery safely', () => {
  const page = fixture({query: 'ember', dimension: '3D'});
  page.controls['search-all-dimensions'].focus();
  const query = '<img src=x onerror=alert(1)>';
  page.search(query);
  assert.equal(page.controls['search-all-dimensions'].hidden, true);
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  assert.equal(page.controls['empty-description'].textContent, `No examples matching “${query}” in 3D. Try fewer search words or clear all filters.`);
  assert.match(page.controls['result-count'].textContent, /Showing 0 of 15 examples/);
  assert.equal(page.controls['empty-description'].children.size, 0, 'query is plain text');
});

test('result and empty-state Escape return to search without changing filters', () => {
  const page = fixture({url: 'https://example.test/?q=physics&dimension=3D'});
  const search = page.controls['catalog-search'];
  page.send(page.controls['catalog-filters'], 'submit');
  const result = page.document.activeElement;
  assert.equal(page.send(result, 'keydown', {key: 'Escape', ctrlKey: true}).defaultPrevented, false);
  assert.equal(page.document.activeElement, result);
  assert.equal(page.send(result, 'keydown', {key: 'Escape'}).defaultPrevented, true);
  assert.equal(page.document.activeElement, search);
  assert.equal(search.value, 'physics');
  assert.equal(page.dimensions[2].checked, true);
  page.search('nothing');
  page.send(page.controls['catalog-filters'], 'submit');
  const escape = page.send(page.controls['catalog-empty'], 'keydown', {key: 'Escape'});
  assert.equal(escape.defaultPrevented, true);
  assert.equal(page.document.activeElement, search);
  assert.equal(search.value, 'nothing');
  page.send(search, 'keydown', {key: 'Escape'});
  assert.equal(search.value, '');
  assert.equal(page.view.location.searchParams.has('q'), false);
  assert.equal(page.view.location.searchParams.get('dimension'), '3D');
});

test('composition does not filter interim text, publish a URL or move focus on submit', () => {
  const page = fixture({url: 'https://example.test/?q=ember'});
  const search = page.controls['catalog-search'];
  search.focus();
  const message = page.controls['result-count'].textContent;
  page.send(search, 'compositionstart');
  search.value = 'phys';
  page.send(search, 'input', {isComposing: true});
  page.send(search, 'input');
  assert.equal(page.send(search, 'keydown', {key: 'Escape'}).defaultPrevented, false);
  assert.equal(search.value, 'phys');
  page.send(page.controls['catalog-filters'], 'submit');
  assert.equal(page.document.activeElement, search);
  assert.deepEqual(page.visible(), [page.links[2]]);
  assert.equal(page.controls['result-count'].textContent, message);
  assert.equal(page.replacements.length, 0);
  search.value = 'physics lab';
  page.send(search, 'compositionend');
  page.send(search, 'input');
  assert.equal(page.visible().length, 2);
  assert.equal(page.replacements.length, 1);
  assert.equal(page.view.location.searchParams.get('q'), 'physics lab');
  assert.equal(page.document.activeElement, search);
  for (const properties of [{isComposing: true}, {keyCode: 229}, {altKey: true}, {ctrlKey: true}, {metaKey: true}, {shiftKey: true}]) {
    assert.equal(page.send(search, 'keydown', {key: 'Escape', ...properties}).defaultPrevented, false);
    assert.equal(search.value, 'physics lab');
  }
});

test('radio changes retain native focus and result announcements distinguish equal counts', () => {
  const page = fixture();
  page.dimensions[2].focus();
  page.select('3D');
  assert.equal(page.document.activeElement, page.dimensions[2]);
  assert.equal(page.send(page.dimensions[2], 'keydown', {key: 'ArrowLeft'}).defaultPrevented, false);
  const count = page.controls['result-count'];
  page.search('crystal');
  assert.match(count.textContent, /Showing 1 of 15 examples matching “crystal” in 3D/);
  const firstWrites = count.textWrites;
  page.search('dune');
  assert.match(count.textContent, /Showing 1 of 15 examples matching “dune” in 3D/);
  assert.equal(count.textWrites, firstWrites + 1);
  page.search('dune');
  assert.equal(count.textWrites, firstWrites + 1, 'unchanged result text is not announced repeatedly');
});

test('late module enhancement reveals the form before rescuing a focused game link', () => {
  const page = fixture({url: 'https://example.test/?q=ember', prepare({controls, links}) {
    links[0].focus();
    const search = controls['catalog-search'];
    const focus = search.focus.bind(search);
    search.focus = () => {
      assert.equal(controls['catalog-filters'].hidden, false, 'focus destination is visible');
      focus();
    };
  }});
  assert.equal(page.document.activeElement, page.controls['catalog-search']);
  assert.deepEqual(page.visible(), [page.links[2]]);
});

test('history restoration reconciles late browser autofill with only one pending callback', () => {
  const callbacks = new Map();
  let nextId = 0;
  const page = fixture({url: 'https://example.test/?q=ember', prepare({document}) {
    document.defaultView.setTimeout = (callback, delay) => {
      assert.equal(delay, 0);
      callbacks.set(++nextId, callback);
      return nextId;
    };
    document.defaultView.clearTimeout = id => callbacks.delete(id);
  }});
  page.send(page.view, 'pageshow', {persisted: true});
  page.view.location = new URL('https://example.test/?q=physics+lab&dimension=3D');
  page.send(page.view, 'popstate');
  assert.equal(callbacks.size, 1);
  // Simulate persisted form state applied by the browser after the event.
  page.controls['catalog-search'].value = 'stale browser value';
  for (const input of page.dimensions) input.checked = input.value === '2D';
  const callback = [...callbacks.values()][0];
  callbacks.clear();
  callback();
  assert.equal(page.controls['catalog-search'].value, 'physics lab');
  assert.equal(page.dimensions[2].checked, true);
  assert.deepEqual(page.visible(), [page.links[14]]);
  assert.equal(page.replacements.length, 0);
});
