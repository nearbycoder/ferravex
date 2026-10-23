// Source/structure and browser-event contract checks; no staged Wasm or visual playtest.
// Run from any directory: node web/tests/journey200-sky-courier-host.mjs
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const hostUrl = new URL('../sky-courier/index.html', import.meta.url);
const html = readFileSync(hostUrl, 'utf8');
const script = html.match(/<script\b[^>]*id="flight-host-accessibility"[^>]*>([\s\S]*?)<\/script>/)?.[1];
const css = html.match(/<style>([\s\S]*?)<\/style>/)?.[1];
assert.ok(script, 'the host accessibility adapter must be exercised');
assert.ok(css);

// Small first-party DOM/event fixture for this host, deliberately without layout
// or native focus simulation. Events bubble to the real shared input adapter.
class Element {
  constructor(tag, attrs = {}) {
    this.tag = tag;
    this.attrs = new Map(Object.entries(attrs));
    this.children = [];
    this.listeners = new Map();
    this.observers = [];
    this.writes = 0;
  }
  get textContent() { return this.children.map(child => typeof child === 'string' ? child : child.textContent).join(''); }
  getAttribute(name) { return this.attrs.get(name) ?? null; }
  hasAttribute(name) { return this.attrs.has(name); }
  setAttribute(name, value) {
    this.attrs.set(name, String(value));
    this.writes++;
    this.changed(name);
  }
  removeAttribute(name) { this.attrs.delete(name); this.changed(name); }
  changed(name) {
    for (const observer of this.observers) {
      if (!observer.options.attributeFilter.includes(name) || observer.pending) continue;
      observer.pending = true;
      queueMicrotask(() => { observer.pending = false; observer.callback(); });
    }
  }
  matches(selector) {
    if (selector.startsWith('#')) return this.getAttribute('id') === selector.slice(1);
    const attribute = selector.match(/^\[([^=\]]+)(?:=["']?([^"'\]]+)["']?)?\]$/);
    if (attribute) return attribute[2] === undefined ? this.hasAttribute(attribute[1]) : this.getAttribute(attribute[1]) === attribute[2];
    return this.tag === selector;
  }
  closest(selectors) {
    for (let node = this; node; node = node.parent) {
      if (selectors.split(',').some(selector => node.matches(selector))) return node;
    }
    return null;
  }
  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(fn);
  }
  emit(type, init = {}) {
    const event = {
      target: this, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; }, ...init,
    };
    for (let node = this; node; node = node.parent) {
      for (const fn of node.listeners.get(type) ?? []) fn(event);
      if (event.stopped) break;
    }
    return event;
  }
  getBoundingClientRect() { return {left: 0, top: 0, width: 390, height: 480}; }
}

function documentFixture() {
  const win = new Element('window'), doc = new Element('document'), nodes = [];
  doc.parent = win;
  doc.hasFocus = () => true;
  doc.hidden = false;
  const stack = [doc];
  const markup = html.replace(/<!--[\s\S]*?-->/g, '').replace(/(<(script|style)\b[^>]*>)[\s\S]*?(<\/\2>)/g, '$1$3');
  const voidTags = new Set(['meta', 'link', 'br', 'hr', 'input']);
  for (const token of markup.matchAll(/<(!doctype[^>]*|\/?[a-z][^>]*)>|([^<]+)/gi)) {
    if (token[2]) { stack.at(-1).children.push(token[2]); continue; }
    if (token[1].startsWith('!')) continue;
    if (token[1].startsWith('/')) {
      assert.equal(stack.pop().tag, token[1].slice(1).trim(), 'host markup must nest correctly');
      continue;
    }
    const [, tag, attributes] = token[1].match(/^(\S+)([\s\S]*)$/);
    const attrs = Object.fromEntries([...attributes.matchAll(/([^\s=]+)(?:="([^"]*)")?/g)].map(([, key, value]) => [key, value ?? '']));
    const node = new Element(tag, attrs);
    node.parent = stack.at(-1);
    node.parent.children.push(node);
    nodes.push(node);
    if (!voidTags.has(tag)) stack.push(node);
  }
  assert.equal(stack.length, 1);
  doc.querySelector = selector => nodes.find(node => node.matches(selector)) ?? null;
  doc.getElementById = id => doc.querySelector('#' + id);
  return {win, doc, nodes};
}

function boot() {
  const fixture = documentFixture();
  const {win, doc} = fixture;
  const observers = [];
  class MutationObserver {
    constructor(callback) { this.callback = callback; observers.push(this); }
    observe(node, options) { this.options = options; node.observers.push(this); }
  }
  runInNewContext(script, {document: doc, MutationObserver}, {filename: hostUrl.pathname, timeout: 1000});
  const canvas = doc.getElementById('flight-canvas'), controls = doc.querySelector('[data-controls]');
  const calls = [];
  const exports = {
    ferravex_key: (...args) => calls.push(['key', ...args]),
    ferravex_blur: () => calls.push(['blur']),
    ferravex_focus: value => calls.push(['focus', value]),
    ferravex_controls_open: () => controls.getAttribute('aria-pressed') === 'true',
  };
  bindInput(canvas, 'sky-courier', () => exports, () => true, {
    window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  return {...fixture, canvas, controls, calls, observers};
}

const text = node => node.textContent.replace(/\s+/g, ' ').trim();
const describedIds = canvas => canvas.getAttribute('aria-describedby').split(/\s+/);
const flushMutations = async () => { for (let i = 0; i < 4; i++) await Promise.resolve(); };

test('canvas has a named, visible flight guide, fallback, landmarks and resolvable relationships', () => {
  const {doc, nodes} = documentFixture();
  assert.equal(doc.querySelector('html').getAttribute('lang'), 'en');
  assert.equal(text(doc.querySelector('title')), 'Sky Courier · Ferravex');
  assert.equal(nodes.filter(node => node.tag === 'h1').length, 1);
  assert.equal(text(doc.querySelector('h1')), 'Sky Courier');
  const ids = nodes.map(node => node.getAttribute('id')).filter(Boolean);
  assert.equal(new Set(ids).size, ids.length, 'IDs must be unique');
  for (const node of nodes) {
    for (const attribute of ['aria-describedby', 'aria-labelledby']) {
      for (const id of (node.getAttribute(attribute) ?? '').split(/\s+/).filter(Boolean)) {
        assert.ok(doc.getElementById(id), `${attribute} points to missing ${id}`);
      }
    }
    if (node.getAttribute('href')?.startsWith('#')) assert.ok(doc.getElementById(node.getAttribute('href').slice(1)));
  }
  const canvas = doc.querySelector('canvas');
  assert.equal(canvas.getAttribute('tabindex'), '0');
  assert.match(canvas.getAttribute('aria-label'), /Sky Courier.*flight/);
  assert.match(text(canvas), /visual flight game.*guide/);
  assert.equal(canvas.getAttribute('role'), null, 'do not force application mode or an image-only role');
  for (const id of describedIds(canvas)) {
    const description = doc.getElementById(id);
    assert.ok(text(description).length > 20);
    assert.equal(description.closest('[hidden],[inert],[aria-hidden="true"]'), null);
  }
  assert.equal(doc.getElementById('flight-guide').getAttribute('tabindex'), '-1');
  assert.equal(nodes.filter(node => node.getAttribute('role') === 'status').length, 1);
  const status = doc.getElementById('flight-status');
  assert.equal(status.getAttribute('aria-live'), 'polite');
  assert.equal(status.getAttribute('aria-atomic'), 'true');
  const viewport = nodes.find(node => node.getAttribute('name') === 'viewport').getAttribute('content');
  assert.doesNotMatch(viewport, /user-scalable\s*=\s*no|maximum-scale/);
  assert.match(text(doc.querySelector('noscript')), /Enable JavaScript/);
});

test('default flight instructions match current gameplay and explicitly defer to saved controls', () => {
  const {doc} = documentFixture();
  const guide = text(doc.getElementById('flight-guide'));
  assert.match(guide, /five amber gates in order.*18 metres per second.*90 seconds/);
  assert.match(guide, /Saved or remapped controls take precedence.*keys below are defaults/);
  assert.match(guide, /F2.*current bindings.*when browser storage is available/);
  assert.match(guide, /Default keyboard controls.*Enter.*A \/ D.*Left \/ Right.*W \/ S.*Up \/ Down.*Space.*either Shift.*Escape.*R restarts/s);
  assert.match(guide, /Release held flight controls after starting or retrying/);
  assert.match(guide, /Escape cancels a binding capture first, then closes Controls/);
  assert.match(guide, /Fullscreen may use the first Escape/);
  assert.doesNotMatch(guide, /touch|tap|swipe/i, 'do not promise unverified touch controls');
  const game = readFileSync(new URL('../../examples/sky-courier/src/lib.rs', import.meta.url), 'utf8');
  assert.match(game, /PlayerInput::preset\("sky-courier", Preset::Flight\)/);
  for (const action of ['boost', 'brake']) assert.ok(game.includes(`player_input.held("${action}")`));
  assert.ok(game.includes('player_input.pressed("restart")'));
  assert.match(game, /const DELIVERY_SPEED: f32 = 18\./);
  const presets = readFileSync(new URL('../../crates/ferravex-render/src/input/presets.rs', import.meta.url), 'utf8');
  for (const pair of ['KeyCode::A, KeyCode::Left', 'KeyCode::D, KeyCode::Right', 'KeyCode::W, KeyCode::Up', 'KeyCode::S, KeyCode::Down']) assert.ok(presets.includes(pair));
  assert.match(presets, /preset == Preset::Flight[^]*?KeyCode::LeftShift[^]*?KeyCode::RightShift[^]*?add\("brake"/);
  assert.match(presets, /key\(KeyCode::Space\)[^]*?add\("boost"/);
  assert.match(presets, /"restart",[^]*?key\(KeyCode::R\)/);
  assert.match(presets, /"confirm",[^]*?key\(KeyCode::Enter\)/);
});

test('source routes keep the shared runtime, game identity, boot ABI and player tools', () => {
  const {doc, nodes} = documentFixture();
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const game = catalog.find(entry => entry.id === 'sky-courier');
  assert.equal(game.host, 'sky-courier');
  assert.equal(doc.querySelector('canvas').getAttribute('data-game'), game.id);
  const modules = nodes.filter(node => node.tag === 'script' && node.hasAttribute('src'));
  assert.equal(modules.length, 1);
  assert.equal(modules[0].getAttribute('type'), 'module');
  assert.equal(new URL(modules[0].getAttribute('src'), hostUrl).href, new URL('../runtime.js', import.meta.url).href);
  assert.equal(nodes.find(node => node.tag === 'a' && text(node) === 'All games').getAttribute('href'), '../');
  assert.ok(existsSync(new URL('../index.html', hostUrl)), 'All games route exists in source');
  assert.equal(nodes.filter(node => node.tag === 'button').length, 2, 'shared player controls must not be duplicated');
  for (const selector of ['[data-controls]', '[data-audio]']) {
    const button = doc.querySelector(selector);
    assert.equal(button.getAttribute('type'), 'button');
    assert.equal(button.getAttribute('aria-pressed'), 'false');
    assert.ok(button.hasAttribute('disabled'), 'the shared runtime enables controls when ready');
  }
  const runtime = readFileSync(new URL('../runtime.js', import.meta.url), 'utf8');
  assert.ok(runtime.includes("loadWasm(`${game.replaceAll('-','_')}.wasm`,imports)"));
  assert.equal(new URL(game.id.replaceAll('-', '_') + '.wasm', new URL('https://example.test/web/sky-courier/')).pathname, '/web/sky-courier/sky_courier.wasm');
  for (const hook of ['exports.ferravex_init()', 'exports.ferravex_frame(', 'bindInput(', 'bindDetails(', 'bindAudio(', 'bindGraphicsLifecycle(', 'new RetainedRenderer(', 'new SpriteRenderer(']) assert.ok(runtime.includes(hook), hook);
  assert.ok(readFileSync(new URL('../details.js', import.meta.url), 'utf8').includes('bindPlayer(canvas,game,getExports,body,'));
  assert.ok(readFileSync(new URL('../../examples/sky-courier/src/lib.rs', import.meta.url), 'utf8').includes('ferravex_render::game!(conf, game);'));
  assert.doesNotMatch(script, /fetch\(|WebAssembly|requestAnimationFrame|ferravex_(?:init|key|frame)|localStorage/);
});

test('responsive source styles protect wrapping navigation, instructions, focus and shared status messages', () => {
  assert.match(css, /#flight-nav\s*\{[^}]*flex-wrap:\s*wrap[^}]*height:\s*auto/);
  assert.match(css, /#flight-nav\s*>\s*a\s*\{[^}]*display:\s*inline-flex/);
  assert.match(css, /#flight-nav\s*>\s*button\s*\{[^}]*min-height:\s*44px[^}]*font:\s*inherit/);
  assert.match(css, /:focus-visible\s*\{[^}]*outline:\s*3px/);
  assert.match(css, /#flight-canvas:focus-visible\s*\{[^}]*outline:[^}]*outline-offset:\s*-4px/);
  assert.match(css, /@media\s*\(max-width:\s*540px\)[^]*?grid-template-columns:\s*minmax\(0,\s*1fr\)/);
  assert.match(css, /#flight-guide\s*\{[^}]*overflow-wrap:\s*anywhere/);
  assert.doesNotMatch(css, /outline:\s*(none|0)\b|\[role[=\s]/);
  assert.match(css, /body\.ferravex-fullscreen main[^}]*min-height:\s*0/);
  assert.match(css, /body\.ferravex-fullscreen main[^}]*overflow-y:\s*auto/);
  assert.doesNotMatch(css, /#flight-guide\s*\{[^}]*display:\s*none/, 'the guide link must stay usable in fullscreen');
});

test('shared description replacement keeps both help sources and settles without an observer loop', async () => {
  const {canvas, observers} = boot();
  const original = describedIds(canvas);
  assert.equal(observers.length, 1);
  assert.equal(canvas.writes, 0, 'already-correct markup needs no rewrite');
  canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
  await flushMutations();
  assert.deepEqual(describedIds(canvas), ['ferravex-canvas-help', ...original]);
  assert.equal(canvas.writes, 2, 'one shared write, one repair; repair does not loop');
  canvas.setAttribute('aria-describedby', 'ferravex-canvas-help flight-controls future-shared-help');
  await flushMutations();
  const current = describedIds(canvas);
  assert.equal(new Set(current).size, current.length);
  for (const id of [...original, 'ferravex-canvas-help', 'future-shared-help']) assert.ok(current.includes(id));
  canvas.setAttribute('aria-label', 'Sky Courier game canvas');
  await flushMutations();
  assert.equal(canvas.getAttribute('aria-label'), 'Sky Courier game canvas', 'shared guide labeling remains owned by Player tools');
  canvas.removeAttribute('aria-describedby');
  await flushMutations();
  assert.deepEqual(describedIds(canvas), original);
});

test('Tab can leave the game; gameplay, Escape and F2 still reach shared input exactly once', () => {
  const {canvas, calls} = boot();
  for (const shiftKey of [false, true]) {
    const event = canvas.emit('keydown', {code: 'Tab', key: 'Tab', shiftKey});
    assert.equal(event.defaultPrevented, false, 'browser retains native Tab focus navigation');
    assert.equal(calls.length, 0, 'page navigation must not enter gameplay');
  }
  for (const [code, key, wasmCode] of [['Enter', 'Enter', 0xff0d], ['Escape', 'Escape', 0xff1b], ['F2', 'F2', 0xffbf], ['KeyR', 'r', 114], ['KeyW', 'w', 119], ['Space', ' ', 32], ['ShiftRight', 'Shift', 0xffe2]]) {
    calls.length = 0;
    assert.equal(canvas.emit('keydown', {code, key}).defaultPrevented, true);
    canvas.emit('keyup', {code, key});
    assert.deepEqual(calls, [['key', wasmCode, 1], ['key', wasmCode, 0]]);
  }
});

test('an open renderer Controls panel retains Tab/arrows/Enter and Escape/F2 handling', () => {
  const {canvas, controls, calls} = boot();
  controls.setAttribute('aria-pressed', 'true');
  for (const [code, wasmCode] of [['Tab', 0xff09], ['ArrowDown', 0xff54], ['ArrowUp', 0xff52], ['Enter', 0xff0d], ['Escape', 0xff1b], ['F2', 0xffbf]]) {
    calls.length = 0;
    assert.equal(canvas.emit('keydown', {code, key: code}).defaultPrevented, true);
    canvas.emit('keyup', {code, key: code});
    assert.deepEqual(calls, [['key', wasmCode, 1], ['key', wasmCode, 0]]);
  }
  controls.setAttribute('aria-pressed', 'false');
  calls.length = 0;
  assert.equal(canvas.emit('keydown', {code: 'Tab', key: 'Tab'}).defaultPrevented, false);
  assert.deepEqual(calls, []);
});

test('page links and guide keep native key behavior while releases clear held flight input', () => {
  const {doc, nodes, canvas, calls} = boot();
  const targets = [...nodes.filter(node => node.tag === 'a'), doc.getElementById('flight-guide'), doc.querySelector('[data-controls]')];
  for (const target of targets) {
    for (const code of ['Enter', 'Space', 'ArrowDown', 'Tab']) {
      calls.length = 0;
      const event = target.emit('keydown', {code, key: code});
      assert.equal(event.defaultPrevented, false);
      assert.deepEqual(calls, [], 'browsing/activating the guide must not start or move the aircraft');
    }
  }
  canvas.emit('keydown', {code: 'KeyW', key: 'w'});
  targets[0].emit('keyup', {code: 'KeyW', key: 'w'});
  assert.deepEqual(calls, [['key', 119, 1], ['key', 119, 0]], 'releasing over a link must not leave climb held');
});

test('existing window focus lifecycle remains owned by shared input', () => {
  const {win, calls} = boot();
  win.emit('blur');
  win.emit('focus');
  assert.deepEqual(calls, [['blur'], ['focus', 1]], 'no host-local pause or extra lifecycle injection');
});
