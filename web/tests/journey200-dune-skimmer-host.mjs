// Source/structure and DOM event contracts only. No staged Wasm, GPU, layout,
// assistive technology, native Tab navigation or device playtest is exercised.
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';
import {bindInput} from '../input.js';

const hostURL = new URL('../dune-skimmer/index.html', import.meta.url);
const html = readFileSync(hostURL, 'utf8');
const runtime = readFileSync(new URL('../runtime.js', import.meta.url), 'utf8');
const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
const attributes = source => Object.fromEntries([...source.matchAll(/([\w:-]+)(?:\s*=\s*"([^"]*)")?/g)].map(([, name, value]) => [name, value ?? '']));

// A deliberately small first-party DOM double for this host's ordinary HTML.
// Parsing the actual source keeps event targets, IDs and ancestry under test.
function fixture() {
  const observers = [];
  let document;
  class Element {
    constructor(tag, attrs = {}, parent = null) {
      this.tagName = tag;
      this.attrs = attrs;
      this.parent = parent;
      this.children = [];
      this.listeners = new Map();
      this.content = [];
      parent?.children.push(this);
      parent?.content.push(this);
    }
    getAttribute(name) { return this.attrs[name] ?? null; }
    setAttribute(name, value) {
      this.attrs[name] = value;
      for (const observer of observers) {
        if (observer.target === this && observer.options.attributeFilter.includes(name)) observer.pending = true;
      }
    }
    addEventListener(type, callback) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(callback);
    }
    focus() { document.activeElement = this; }
    get textContent() { return this.content.map(part => typeof part === 'string' ? part : part.textContent).join(''); }
    closest(selector) {
      const tags = selector.split(',');
      for (let node = this; node; node = node.parent) if (tags.includes(node.tagName)) return node;
      return null;
    }
  }
  const window = new Element('window');
  document = new Element('document', {}, window);
  document.hasFocus = () => true;
  document.hidden = false;
  const elements = [];
  const stack = [document];
  const source = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/g, '');
  const voidTags = new Set(['meta', 'link', 'br', 'hr', 'input']);
  for (const [token] of source.matchAll(/<!--[\s\S]*?-->|<![^>]*>|<[^>]+>|[^<]+/g)) {
    if (token.startsWith('<!')) continue;
    if (token.startsWith('</')) {
      assert.equal(stack.pop().tagName, token.slice(2, -1).trim(), 'host HTML is properly nested');
    } else if (token.startsWith('<')) {
      const [, tag, attrs] = token.match(/^<([\w-]+)([^>]*)>/);
      const node = new Element(tag, attributes(attrs), stack.at(-1));
      elements.push(node);
      if (!voidTags.has(tag)) stack.push(node);
    } else stack.at(-1).content.push(token);
  }
  assert.equal(stack.length, 1);
  const ids = elements.filter(node => node.attrs.id).map(node => node.attrs.id);
  assert.equal(ids.length, new Set(ids).size, 'every ID is unique');
  document.getElementById = id => elements.find(node => node.attrs.id === id);
  document.activeElement = elements.find(node => node.tagName === 'body');
  const get = id => document.getElementById(id);
  const canvas = get('game-canvas');
  const calls = [];
  const exports = {};
  for (const name of ['key', 'blur', 'focus', 'input_settings']) exports['ferravex_' + name] = (...args) => calls.push([name, ...args]);
  const bridge = bindInput(canvas, 'dune-skimmer', () => exports, () => true, {window, document, navigator:{}, storage:() => ({getItem:() => null})});
  class MutationObserver {
    constructor(callback) { this.callback = callback; observers.push(this); }
    observe(target, options) { this.target = target; this.options = options; }
  }
  function enhance() {
    for (const [, attrs, body] of scripts) if (!('src' in attributes(attrs))) runInNewContext(body, {document, MutationObserver});
  }
  function flush() {
    // A callback that rewrites the same attribute indefinitely is a regression.
    for (let round = 0; round < 5; round++) {
      const pending = observers.filter(observer => observer.pending);
      if (!pending.length) return;
      for (const observer of pending) { observer.pending = false; observer.callback(); }
    }
    assert.fail('description observer did not settle');
  }
  function send(target, type, fields = {}) {
    const event = {target, defaultPrevented:false, stopped:false, ...fields,
      preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; }};
    for (let node = target; node; node = node.parent) {
      for (const callback of node.listeners.get(type) ?? []) callback(event);
      if (event.stopped) break;
    }
    return event;
  }
  return {get, elements, document, canvas, calls, bridge, enhance, flush, send};
}

test('canvas has a meaningful name, visible descriptions, fallback and a focusable guide', () => {
  const h = fixture();
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>Dune Skimmer · Ferravex<\/title>/);
  assert.equal(h.elements.filter(node => node.tagName === 'main').length, 1);
  assert.equal(h.elements.filter(node => node.tagName === 'h1').length, 1);
  assert.equal(h.elements.filter(node => node.tagName === 'canvas').length, 1);
  assert.equal(h.canvas.getAttribute('tabindex'), '0');
  assert.equal(h.canvas.getAttribute('aria-label'), 'Dune Skimmer game canvas');
  assert.match(h.canvas.textContent, /WebGL2/);
  for (const id of h.canvas.getAttribute('aria-describedby').split(' ')) {
    const description = h.get(id);
    assert.ok(description?.textContent.trim().length > 40, `${id} describes real behavior`);
    assert.equal(description.getAttribute('hidden'), null);
    assert.equal(description.getAttribute('aria-hidden'), null);
    assert.equal(description.getAttribute('style'), null, 'instructions are not clipped or hidden');
    assert.equal(description.parent, h.get('race-guide'));
  }
  assert.match(h.get('race-help').textContent, /three laps of fourteen ordered gates, for 42 gate passes/);
  assert.match(h.get('binding-help').textContent, /Saved or remapped bindings may differ/);
  assert.equal(h.get('race-guide-title').getAttribute('tabindex'), '-1');
  assert.equal(h.get('load-status').getAttribute('role'), 'status');
  assert.equal(h.get('load-status').getAttribute('aria-live'), 'polite');
  assert.equal(h.get('load-status').getAttribute('aria-atomic'), 'true');
  assert.ok(h.elements.some(node => node.tagName === 'noscript'));
  for (const node of h.elements) {
    for (const attribute of ['aria-labelledby', 'aria-describedby']) {
      for (const id of (node.getAttribute(attribute) ?? '').split(' ').filter(Boolean)) assert.ok(h.get(id), `${attribute} resolves ${id}`);
    }
  }
});

test('defaults and navigation guidance distinguish remapping, pause, editor capture and focus', () => {
  const h = fixture();
  const rows = h.elements.filter(node => node.tagName === 'dt').map(node => (node.textContent + ' ' + node.parent.children.find(child => child.tagName === 'dd').textContent).replace(/\s+/g, ' ').trim());
  assert.equal(rows.length, 6);
  for (const expected of [/Accelerate W or Up arrow/, /Brake \/ reverse S or Down arrow/, /Steer A \/ D or Left \/ Right arrows/, /Boost Space or Shift, while accelerating/, /Pause \/ resume Escape/, /Restart race R/]) {
    assert.ok(rows.some(row => expected.test(row)), `${expected} is documented`);
  }
  const help = h.get('race-guide').textContent.replace(/\s+/g, ' ');
  assert.match(help, /countdown starts when the game loads/);
  assert.match(help, /Ctrl \+ F6/);
  assert.match(help, /Moving to this guide does not pause the race/);
  assert.match(help, /Escape cancels a pending binding capture/);
  assert.match(help, /current Pause binding to resume/);
  assert.match(help, /Details → Player tools/);
  assert.doesNotMatch(help, /touch controls|swipe|tap to (?:start|play)/i, 'no additional touch capability is advertised');
});

test('source CSS preserves readable wrapping, focus, navigation and status boundaries', () => {
  const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
  assert.match(css, /#game-nav\{[^}]*height:auto;[^}]*flex-wrap:wrap/);
  assert.match(css, /#game-nav>a\{[^}]*display:inline-flex;[^}]*min-height:44px;[^}]*white-space:normal/);
  assert.match(css, /#game-nav button\{[^}]*min-height:44px;[^}]*font:inherit/);
  assert.match(css, /#game-nav :focus-visible,#race-guide :focus-visible\{outline:3px solid/);
  assert.match(css, /#game-canvas:focus-visible\{outline:3px solid[^}]*!important/);
  assert.match(css, /@media\(max-width:36rem\)[\s\S]*\.race-keys div\{grid-template-columns:1fr/);
  assert.match(css, /body#dune-host\.ferravex-fullscreen\{overflow-y:auto\}/);
  assert.match(css, /@media\(forced-colors:active\)/);
  assert.doesNotMatch(css, /(?:^|\})\s*\[role[=:]/, 'loading styles cannot reposition shared player status messages');
  assert.doesNotMatch(css, /outline:none|overflow-x:hidden|clip-path|white-space:nowrap/);
});

test('catalog, relative routes and the single runtime preserve loader, ABI and renderer hooks', () => {
  const h = fixture();
  const game = catalog.find(entry => entry.id === h.canvas.getAttribute('data-game'));
  assert.equal(game?.host, 'dune-skimmer');
  assert.equal(game?.title, 'Dune Skimmer');
  const external = scripts.map(([, source]) => attributes(source)).filter(attrs => attrs.src);
  assert.deepEqual(external, [{type:'module', src:'../runtime.js'}]);
  assert.ok(existsSync(new URL(external[0].src, hostURL)));
  assert.equal(h.elements.filter(node => 'data-controls' in node.attrs).length, 1);
  assert.equal(h.elements.filter(node => 'data-audio' in node.attrs).length, 1);
  for (const key of ['data-controls', 'data-audio']) {
    const button = h.elements.find(node => key in node.attrs);
    assert.equal(button.tagName, 'button');
    assert.equal(button.getAttribute('type'), 'button');
    assert.ok('disabled' in button.attrs, 'runtime owns readiness');
    assert.equal(button.getAttribute('aria-pressed'), 'false');
  }
  assert.equal(h.elements.filter(node => node.tagName === 'button').length, 2, 'global player controls stay shared');
  for (const prefix of ['/web/', '/play/']) {
    const route = new URL(`${prefix}${game.host}/`, 'https://ferravex.test');
    assert.equal(new URL(h.get('catalog-link').attrs.href, route).pathname, prefix);
    assert.equal(new URL(external[0].src, route).pathname, prefix + 'runtime.js');
    assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, route).pathname, prefix + 'dune-skimmer/dune_skimmer.wasm');
    for (const link of h.elements.filter(node => node.tagName === 'a' && node.attrs.href.startsWith('#'))) {
      const target = new URL(link.attrs.href, route);
      assert.equal(target.pathname, route.pathname);
      assert.ok(h.get(target.hash.slice(1)), 'local navigation resolves');
    }
  }
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-',\s*'_'\)\}\.wasm`,imports\)/);
  for (const hook of ['exports.ferravex_init()', 'exports.ferravex_autoplay(', 'exports.ferravex_frame(', 'bindGraphicsLifecycle(canvas', 'bindDetails(canvas', 'bindAudio(canvas', "import('./material-tools.js')", 'new RetainedRenderer(gl)', 'new SpriteRenderer(gl)']) assert.ok(runtime.includes(hook), hook);
  assert.doesNotMatch(scripts[0][2], /ferravex_|WebAssembly|fetch\(|requestAnimationFrame|localStorage/, 'host enhancement does not invent ABI or simulation paths');
});

test('shared player description replacement retains both help sources and settles without a loop', () => {
  for (const bootFirst of [false, true]) {
    const h = fixture();
    if (bootFirst) h.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
    h.enhance();
    if (!bootFirst) h.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
    h.flush();
    assert.equal(h.canvas.getAttribute('aria-describedby'), 'race-help keyboard-help binding-help ferravex-canvas-help');
    h.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help ferravex-canvas-help');
    h.flush();
    assert.equal(h.canvas.getAttribute('aria-describedby'), 'race-help keyboard-help binding-help ferravex-canvas-help');
    h.canvas.setAttribute('aria-describedby', '');
    h.flush();
    assert.equal(h.canvas.getAttribute('aria-describedby'), 'race-help keyboard-help binding-help');
  }
});

test('canvas exit and page links work without forwarding new gameplay presses', () => {
  const h = fixture(); h.enhance(); h.canvas.focus();
  h.send(h.canvas, 'keydown', {key:'w', code:'KeyW'});
  assert.deepEqual(h.calls, [['key', 119, 1]]);
  const exit = h.send(h.canvas, 'keydown', {key:'F6', code:'F6', ctrlKey:true});
  assert.equal(exit.defaultPrevented, true);
  assert.equal(h.document.activeElement, h.get('catalog-link'));
  assert.equal(h.calls.length, 1, 'exit does not send an action or change the session');
  h.send(h.get('catalog-link'), 'keyup', {key:'w', code:'KeyW'});
  assert.deepEqual(h.calls.at(-1), ['key', 119, 0], 'held gameplay releases still reach the bridge');
  h.calls.length = 0;
  for (const id of ['catalog-link', 'guide-link', 'race-guide-title', 'return-to-game']) {
    for (const [key, code] of [['Tab', 'Tab'], ['Enter', 'Enter'], [' ', 'Space'], ['r', 'KeyR'], ['Escape', 'Escape']]) {
      assert.equal(h.send(h.get(id), 'keydown', {key, code}).defaultPrevented, false, 'native page behavior remains available');
    }
  }
  assert.deepEqual(h.calls, [], 'page navigation and reading never trigger gameplay presses');
  assert.equal(h.send(h.get('guide-link'), 'click').defaultPrevented, false, 'fragment navigation remains native');
  assert.equal(h.document.activeElement, h.get('race-guide-title'));
  h.send(h.get('return-to-game'), 'click');
  assert.equal(h.document.activeElement, h.canvas);
});

test('gameplay, remappable keys, F2, Escape and editor Tab still reach the real bridge', () => {
  const h = fixture(); h.enhance();
  const keys = [['w', 'KeyW', 119], [' ', 'Space', 32], ['Shift', 'ShiftLeft', 0xffe1], ['r', 'KeyR', 114], ['j', 'KeyJ', 106], ['F2', 'F2', 0xffbf], ['Escape', 'Escape', 0xff1b], ['Tab', 'Tab', 0xff09]];
  for (const [key, code, value] of keys) {
    h.calls.length = 0;
    assert.equal(h.send(h.canvas, 'keydown', {key, code}).defaultPrevented, true);
    h.send(h.canvas, 'keyup', {key, code});
    assert.deepEqual(h.calls, [['key', value, 1], ['key', value, 0]]);
  }
  h.calls.length = 0;
  h.bridge.openSettings();
  assert.deepEqual(h.calls, [['input_settings']], 'existing Controls export remains the entry point');
  h.canvas.focus();
  h.send(h.canvas, 'keydown', {key:'F6', code:'F6', ctrlKey:true, isComposing:true});
  assert.equal(h.document.activeElement, h.canvas, 'composition cannot trigger navigation');
});
