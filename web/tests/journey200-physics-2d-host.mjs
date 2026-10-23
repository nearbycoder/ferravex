// Source/structure and DOM event-contract checks. No generated Wasm or browser
// layout is used here; the parent owns fresh builds and visual playtests.
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const hostUrl = new URL('../physics-2d/index.html', import.meta.url);
const html = readFileSync(hostUrl, 'utf8');
const runtime = readFileSync(new URL('../runtime.js', import.meta.url), 'utf8');
const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
const script = html.match(/<script id="lab-host-accessibility">([\s\S]*?)<\/script>/)?.[1];
const css = html.match(/<style>([\s\S]*?)<\/style>/)?.[1];
const text = value => value.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();

// A deliberately small fixture for this HTML's tags, attributes, bubbling and
// attribute observers. It does not emulate native Tab order or accessible names.
function page() {
  const pending = new Set();
  const nodes = [];
  class Element {
    constructor(tag, attrs = {}, parent = null) {
      this.tag = tag;
      this.attrs = new Map(Object.entries(attrs));
      this.parent = parent;
      this.listeners = new Map();
      this.observers = [];
    }
    getAttribute(name) { return this.attrs.get(name) ?? null; }
    setAttribute(name, value) {
      this.attrs.set(name, String(value));
      for (const observer of this.observers) {
        if (observer.options.attributeFilter.includes(name)) pending.add(observer);
      }
    }
    matches(selector) {
      if (selector.startsWith('#')) return this.getAttribute('id') === selector.slice(1);
      if (selector.startsWith('[')) return this.attrs.has(selector.slice(1, -1));
      return this.tag === selector;
    }
    closest(selectors) {
      for (let node = this; node; node = node.parent) {
        if (selectors.split(',').some(selector => node.matches(selector))) return node;
      }
      return null;
    }
    addEventListener(type, listener) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(listener);
    }
    focus() { document.activeElement = this; }
    getBoundingClientRect() { return {left: 0, top: 0}; }
  }
  const window = new Element('window');
  const document = new Element('document', {}, window);
  document.hidden = false;
  document.hasFocus = () => true;
  document.querySelectorAll = selector => nodes.filter(node => node.matches(selector));
  document.querySelector = selector => document.querySelectorAll(selector)[0] ?? null;
  const stack = [document];
  const markup = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/g, '');
  for (const match of markup.matchAll(/<(\/?)([a-z][\w-]*)\b([^>]*)>/gi)) {
    const [, closing, tag, raw] = match;
    if (closing) {
      assert.equal(stack.pop().tag, tag, `balanced ${tag} markup`);
      continue;
    }
    const attrs = {};
    for (const attr of raw.matchAll(/([\w-]+)(?:="([^"]*)")?/g)) attrs[attr[1]] = attr[2] ?? '';
    const element = new Element(tag, attrs, stack.at(-1));
    nodes.push(element);
    if (!['meta', 'link', 'br', 'hr', 'input'].includes(tag)) stack.push(element);
  }
  assert.equal(stack.length, 1);
  class MutationObserver {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { this.options = options; target.observers.push(this); }
  }
  const flush = () => {
    let rounds = 0;
    while (pending.size) {
      assert.ok(++rounds <= 3, 'description reconciliation must settle, not loop');
      const batch = [...pending];
      pending.clear();
      for (const observer of batch) observer.callback();
    }
  };
  const send = (target, type, fields = {}) => {
    const event = {
      target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; },
      ...fields,
    };
    for (let node = target; node; node = node.parent) {
      for (const listener of node.listeners.get(type) ?? []) listener(event);
      if (event.stopped) break;
    }
    return event;
  };
  assert.ok(script, 'host enhancement is present');
  runInNewContext(script, {document, MutationObserver});
  return {nodes, document, window, flush, send, canvas: document.querySelector('canvas'), controls: document.querySelector('[data-controls]')};
}

test('canvas has a stable title, visible description, fallback and native focus target', () => {
  const p = page();
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>2D Physics Lab · Ferravex<\/title>/);
  assert.equal(p.nodes.filter(node => node.tag === 'canvas').length, 1);
  assert.equal(p.canvas.getAttribute('tabindex'), '0');
  assert.equal(p.canvas.getAttribute('aria-labelledby'), 'lab-title');
  const ids = p.nodes.map(node => node.getAttribute('id')).filter(Boolean);
  assert.equal(new Set(ids).size, ids.length, 'IDs are unique');
  for (const id of ['lab-title', ...p.canvas.getAttribute('aria-describedby').split(' ')]) {
    const node = p.document.querySelector('#' + id);
    assert.ok(node, `${id} resolves`);
    for (let ancestor = node; ancestor; ancestor = ancestor.parent) {
      assert.equal(ancestor.getAttribute('hidden'), null);
      assert.notEqual(ancestor.getAttribute('aria-hidden'), 'true');
      assert.notEqual(ancestor.getAttribute('class'), 'sr-only');
    }
    assert.ok(text(html.match(new RegExp(`<[^>]+id="${id}"[^>]*>([\\s\\S]*?)<\\/(?:p|h1)>`))[1]).length > 10);
  }
  assert.match(html, /<canvas[^>]*>[\s\S]*?WebGL2[\s\S]*?<\/canvas>/);
  assert.match(html, /<noscript>[\s\S]*?JavaScript and WebGL2/);
  assert.match(html, /id="lab-status" role="status" aria-live="polite" aria-atomic="true"/);
});

test('shared help replacement is merged without losing either description or relabelling the title', () => {
  const p = page();
  const authored = p.canvas.getAttribute('aria-describedby');
  // These are the attributes bindPlayer sets during runtime initialization.
  p.canvas.setAttribute('aria-label', 'Physics 2d game canvas');
  p.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
  p.flush();
  assert.equal(p.canvas.getAttribute('aria-labelledby'), 'lab-title');
  assert.equal(p.canvas.getAttribute('aria-describedby'), authored + ' ferravex-canvas-help');
  p.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help lab-summary ferravex-canvas-help');
  p.flush();
  assert.equal(p.canvas.getAttribute('aria-describedby'), authored + ' ferravex-canvas-help');
  p.canvas.setAttribute('aria-describedby', '');
  p.flush();
  assert.equal(p.canvas.getAttribute('aria-describedby'), authored);
});

test('host keeps the catalog route, runtime, player hooks and Wasm naming contract', () => {
  const p = page();
  const game = p.canvas.getAttribute('data-game');
  const entry = catalog.find(entry => entry.id === game);
  assert.equal(entry.host, 'physics-2d');
  assert.equal(entry.title, '2D Physics Lab');
  const sources = [...html.matchAll(/<script[^>]*src="([^"]+)"/g)].map(match => match[1]);
  assert.deepEqual(sources, ['../runtime.js']);
  assert.match(html, /<script type="module" src="\.\.\/runtime.js"><\/script>/);
  assert.ok(existsSync(new URL(sources[0], hostUrl)));
  assert.ok(html.indexOf('id="lab-host-accessibility"') < html.indexOf('src="../runtime.js"'));
  for (const base of ['https://example.test/physics-2d/', 'https://example.test/play/physics-2d/']) {
    const root = new URL('../', base);
    assert.equal(new URL(sources[0], base).href, new URL('runtime.js', root).href);
    assert.equal(new URL(`${game.replaceAll('-', '_')}.wasm`, base).pathname, new URL('physics_2d.wasm', base).pathname);
    for (const link of p.nodes.filter(node => node.tag === 'a')) {
      const destination = new URL(link.getAttribute('href'), base);
      if (destination.hash) assert.ok(p.document.querySelector(destination.hash));
      else assert.equal(destination.href, root.href);
    }
  }
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-','_'\)\}\.wasm`,imports\)/);
  assert.match(runtime, /exports\.ferravex_init\(\)/);
  for (const hook of ['bindInput', 'bindDetails', 'bindAudio', 'bindGraphicsLifecycle']) assert.ok(runtime.includes(hook));
  for (const selector of ['[data-controls]', '[data-audio]']) {
    const matches = p.document.querySelectorAll(selector);
    assert.equal(matches.length, 1);
    assert.equal(matches[0].getAttribute('type'), 'button');
    assert.equal(matches[0].getAttribute('aria-pressed'), 'false');
    assert.equal(matches[0].getAttribute('disabled'), '');
  }
  assert.equal(p.nodes.filter(node => node.tag === 'button').length, 2, 'shared tools are injected, not copied into the host');
  assert.equal(p.controls.parent.getAttribute('class'), 'lab-tools', 'shared Details/Fullscreen insert into a wrapping group');
  assert.equal(p.nodes.find(node => node.tag === 'nav').parent.tag, 'header', 'shared narrow CSS must not hide the catalog link');
});

test('instructions describe actual defaults and remapping without promising touch or an Escape pause menu', () => {
  const copy = text(html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/g, ''));
  for (const phrase of ['starts running immediately', '120 Hz', 'Default keyboard and mouse', 'Saved or remapped controls may differ', 'Controls shows your current bindings', 'Escape cancels a binding capture first', 'Escape also closes Details', 'preserves a manually paused scene', '240-body limit']) assert.ok(copy.includes(phrase), phrase);
  for (const key of ['Enter', 'Space', 'N', 'F', 'I', 'R', '1', '2', 'F2']) assert.ok(html.includes(`<kbd>${key}</kbd>`));
  for (const phrase of ['Inspect the body at the cursor without waking it', 'A miss clears body selection', 'awake/asleep counts', 'last-tick contacts', 'bounded cursor-query hits', 'A successful drop selects the new body', 'Reset clears body inspection while retaining the drop shape and pause state']) assert.ok(copy.includes(phrase), phrase);
  const game = readFileSync(new URL('../../examples/physics-2d/src/lib.rs', import.meta.url), 'utf8');
  assert.match(game, /\("inspect", "Inspect body at cursor", KeyCode::I, 6\)/);
  assert.doesNotMatch(copy, /tap to|touch controls|Escape (?:pauses|opens the game menu)|screen.reader (?:playable|support)/i);
  assert.match(css, /flex-wrap:\s*wrap/);
  assert.match(css, /:focus-visible\s*\{[^}]*outline:\s*3px solid/);
  assert.match(css, /@media \(max-width: 540px\)[\s\S]*grid-template-columns: minmax\(0, 1fr\)/);
  assert.match(css, /body\.physics-lab\.ferravex-fullscreen\s*\{\s*overflow:\s*auto/);
  assert.doesNotMatch(css, /display:\s*none|outline:\s*none/);
});

function inputPage() {
  const p = page();
  const calls = [];
  const exports = {ferravex_key: (...args) => calls.push(args)};
  bindInput(p.canvas, 'physics-2d', () => exports, () => false, {
    window: p.window, document: p.document, navigator: {},
    storage: () => ({getItem: () => null}), atob, btoa,
  });
  return {...p, calls};
}

test('native page navigation and canvas Tab exit survive the real shared keyboard bridge', () => {
  const p = inputPage();
  for (const link of p.nodes.filter(node => node.tag === 'a')) {
    for (const key of ['Enter', 'Tab']) {
      const event = p.send(link, 'keydown', {key, code: key});
      assert.equal(event.defaultPrevented, false, 'native link activation/navigation stays available');
    }
  }
  for (const shiftKey of [false, true]) {
    const event = p.send(p.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey});
    assert.equal(event.defaultPrevented, false, 'Tab and Shift+Tab can leave the closed canvas');
  }
  const instructions = p.document.querySelector('#lab-instructions');
  p.send(instructions, 'keydown', {key: ' ', code: 'Space'});
  assert.deepEqual(p.calls, [], 'reading the page cannot pause or trigger gameplay');
  p.send(p.canvas, 'keydown', {key: 'w', code: 'KeyW'});
  p.send(instructions, 'keyup', {key: 'w', code: 'KeyW'});
  assert.deepEqual(p.calls, [[119, 1], [119, 0]], 'a held game key still releases after focus leaves');
});

test('Controls navigation, capture, Escape, F2 and gameplay stay with the existing bridge', () => {
  const p = inputPage();
  p.controls.setAttribute('aria-pressed', 'true');
  for (const [key, code, wasm] of [['Tab', 'Tab', 0xff09], ['Escape', 'Escape', 0xff1b], ['Enter', 'Enter', 0xff0d], ['F2', 'F2', 0xffbf]]) {
    assert.equal(p.send(p.canvas, 'keydown', {key, code}).defaultPrevented, true);
    p.send(p.canvas, 'keyup', {key, code});
    assert.deepEqual(p.calls.slice(-2), [[wasm, 1], [wasm, 0]]);
  }
  p.controls.setAttribute('aria-pressed', 'false');
  p.send(p.canvas, 'keydown', {key: ' ', code: 'Space'});
  assert.deepEqual(p.calls.at(-1), [32, 1]);
  p.send(p.canvas, 'keydown', {key: 'Tab', code: 'Tab', isComposing: true});
  assert.deepEqual(p.calls.at(-1), [32, 1], 'IME composition does not enter gameplay');
});
