// Source/DOM contract checks only. No dist files, Wasm execution, GPU or browser.
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import {bindInput} from '../input.js';
import {bindPlayer} from '../player.js';

const hostURL = new URL('../physics-3d/index.html', import.meta.url);
const html = readFileSync(hostURL, 'utf8');
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)];
const styles = [...html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)].map(match => match[1]).join('\n');
const markup = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, '').replace(/<!--[\s\S]*?-->/g, '');
const attributes = text => Object.fromEntries([...text.matchAll(/([^\s=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g)]
  .map(([, name, double, single, bare]) => [name, double ?? single ?? bare ?? '']));
const tags = [...markup.matchAll(/<([a-z][\w-]*)\b([^>]*)>/gi)].map(([, tag, attrs]) => ({tag, attrs: attributes(attrs)}));
const one = (predicate, label) => {
  const matches = tags.filter(predicate);
  assert.equal(matches.length, 1, label);
  return matches[0];
};
const canvasTag = one(node => node.tag === 'canvas', 'one runtime canvas');
const controlsTag = one(node => Object.hasOwn(node.attrs, 'data-controls'), 'one shared Controls trigger');
const text = value => value.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
const content = id => {
  const node = one(node => node.attrs.id === id, `unique ${id}`);
  const match = markup.match(new RegExp(`<${node.tag}\\b[^>]*\\bid="${id}"[^>]*>([\\s\\S]*?)<\\/${node.tag}>`));
  assert.ok(match, `text for ${id}`);
  return text(match[1]);
};
const hostScript = scripts.find(([, attrs]) => !Object.hasOwn(attributes(attrs), 'src'))?.[2];

test('the canvas has a persistent accessible name, visible description and native landmarks', () => {
  assert.equal(one(node => node.tag === 'html', 'document language').attrs.lang, 'en');
  assert.match(html, /<title>3D Physics Lab · Ferravex<\/title>/);
  one(node => node.tag === 'main', 'main landmark');
  assert.ok(one(node => node.tag === 'nav', 'navigation landmark').attrs['aria-label']);
  assert.equal(canvasTag.attrs.tabindex, '0');
  assert.equal(canvasTag.attrs['aria-labelledby'], 'lab-title');
  assert.equal(content('lab-title'), '3D Physics Lab');
  assert.ok(!['img', 'application'].includes(canvasTag.attrs.role), 'do not mask interactive content or force application mode');
  const ids = tags.flatMap(node => node.attrs.id ? [node.attrs.id] : []);
  assert.equal(new Set(ids).size, ids.length, 'unique HTML IDs');
  for (const node of tags) {
    for (const relation of ['aria-labelledby', 'aria-describedby']) {
      for (const id of (node.attrs[relation] ?? '').split(/\s+/).filter(Boolean)) assert.ok(content(id), `resolved ${relation}: ${id}`);
    }
    if (node.attrs.tabindex !== undefined) assert.ok(Number(node.attrs.tabindex) <= 0, 'natural tab order');
  }
  for (const id of ['lab-help', 'lab-navigation']) {
    assert.ok(canvasTag.attrs['aria-describedby'].split(' ').includes(id));
    assert.ok(!Object.hasOwn(tags.find(node => node.attrs.id === id).attrs, 'hidden'));
  }
  assert.match(content('lab-help'), /loading yard.*starts automatically at 120 Hz.*no start screen or win condition/);
  assert.match(content('lab-navigation'), /Tab and Shift\+Tab/);
  assert.ok(Object.hasOwn(one(node => node.tag === 'details', 'visible instructions').attrs, 'open'));
  const status = one(node => node.attrs.role === 'status', 'one boot status');
  assert.equal(status.attrs['aria-live'], 'polite');
  assert.equal(status.attrs['aria-atomic'], 'true');
  assert.match(html, /<noscript>.*JavaScript is required/s);
});

test('instructions distinguish default controls, saved mappings and the actual lab lifecycle', () => {
  const guide = text(markup);
  assert.match(content('lab-bindings'), /bindings below are defaults.*Saved or remapped controls can differ.*current bindings/);
  assert.match(content('lab-bindings'), /Escape cancels a binding capture.*Escape or F2 closes Controls/);
  for (const phrase of ['1 drops a crate', '2 drops a ball', 'Enter (Drop selected shape)',
    'Space (Pause / menu)', 'N (Step once while paused)', 'C (Reset orbit view)',
    'R (Restart)', 'F (Impulse pulse)', 'Escape is not the default pause key',
    '1/120 second and stays paused', 'Details → Player tools']) assert.ok(guide.includes(phrase), phrase);
  assert.doesNotMatch(guide, /pinch|swipe|tap to (?:play|start)|press Enter to (?:play|start)/i);
  const game = readFileSync(new URL('../../examples/physics-3d/src/lib.rs', import.meta.url), 'utf8');
  const presets = readFileSync(new URL('../../crates/ferravex-render/src/input/presets.rs', import.meta.url), 'utf8');
  assert.match(game, /PlayerInput::preset\("physics-3d", Preset::Lab3\)/);
  for (const [action, label, key] of [['drop', 'Drop selected shape', 'Enter'], ['step', 'Step once while paused', 'N'], ['reset_view', 'Reset orbit view', 'C']]) {
    assert.match(game, new RegExp(`\\("${action}",\\s*"${label}",\\s*KeyCode::${key},`), `task 143 must coordinate changed ${action} guidance`);
  }
  assert.match(presets, /let pause_key = if matches!\(preset, Preset::Lab2 \| Preset::Lab3\)\s*\{\s*KeyCode::Space/);
  for (const action of ['pause', 'restart', 'select_one', 'select_two']) assert.ok(game.includes(`player_input.pressed("${action}")`));
  // Gameplay pulse is consumed only on a simulation tick; menu/edit actions
  // remain immediate. Native tests exercise zero-tick retention and catch-up.
  assert.ok(game.includes('player_input.take_tick(["pulse"])[0].pressed'));
});

test('catalog, nested route, Wasm filename and shared runtime remain the boot contract', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const entry = catalog.find(game => game.id === canvasTag.attrs['data-game']);
  assert.ok(entry);
  assert.equal(entry.id, 'physics-3d');
  assert.equal(entry.host, 'physics-3d');
  assert.equal(entry.title, content('lab-title'));
  const external = scripts.filter(([, attrs]) => Object.hasOwn(attributes(attrs), 'src'));
  assert.equal(external.length, 1, 'one shared boot module');
  const boot = attributes(external[0][1]);
  assert.deepEqual(boot, {type: 'module', src: '../runtime.js'});
  assert.ok(existsSync(new URL(boot.src, hostURL)));
  const runtime = readFileSync(new URL(boot.src, hostURL), 'utf8');
  assert.ok(runtime.includes("loadWasm(`${game.replaceAll('-','_')}.wasm`,imports)"));
  for (const route of ['https://example.test/physics-3d/', 'https://example.test/play/physics-3d/']) {
    const module = new URL(`${entry.id.replaceAll('-', '_')}.wasm`, route);
    assert.equal(module.pathname, new URL('./physics_3d.wasm', route).pathname);
    assert.equal(new URL(boot.src, route).pathname, new URL('../runtime.js', route).pathname);
  }
  const back = one(node => node.tag === 'a' && node.attrs.href === '../', 'catalog route');
  assert.equal(new URL(back.attrs.href, 'https://example.test/play/physics-3d/').pathname, '/play/');
  one(node => node.tag === 'a' && node.attrs.href === '#lab-canvas', 'return-to-canvas route');
  for (const module of ['loader', 'input', 'details', 'audio', 'lifecycle', 'retained', 'effects', 'visuals']) {
    assert.ok(runtime.includes(`from './${module}.js'`), `keep ${module} hooks`);
    assert.ok(existsSync(new URL(`../${module}.js`, import.meta.url)));
  }
  assert.match(runtime, /controlsButton\?\.addEventListener\('click',openControls\)/);
  assert.match(runtime, /function openControls\(\)\{if\(!disposed&&bootReady\)\{inputPlatform.openSettings\(\);canvas.focus\(\);\}\}/);
  for (const tag of [controlsTag, one(node => Object.hasOwn(node.attrs, 'data-audio'), 'one shared audio trigger')]) {
    assert.equal(tag.attrs.type, 'button');
    assert.equal(tag.attrs['aria-pressed'], 'false');
    assert.ok(Object.hasOwn(tag.attrs, 'disabled'), 'runtime enables controls after load');
  }
  assert.doesNotMatch(hostScript, /WebAssembly|requestAnimationFrame|ferravex_|localStorage/, 'host helper must not duplicate runtime, ABI or saved binding ownership');
});

test('narrow layout wraps tools and instructions, preserves the catalog link and scopes status styling', () => {
  assert.equal(one(node => node.tag === 'nav', 'toolbar').attrs.id, 'lab-toolbar');
  assert.match(styles, /#lab-toolbar\s*\{[^}]*flex-wrap:\s*wrap[^}]*height:\s*auto/);
  assert.match(styles, /#lab-toolbar button\s*\{[^}]*min-height:\s*44px[^}]*font:\s*inherit/);
  assert.match(styles, /@media\s*\(max-width:\s*540px\)[\s\S]*\.lab-guide dl\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)/);
  assert.match(styles, /:focus-visible\s*\{[^}]*outline:\s*3px solid/);
  assert.match(styles, /canvas:focus-visible\s*\{[^}]*outline-offset:\s*-4px/);
  assert.match(styles, /body\.ferravex-fullscreen\s*\{[^}]*overflow:\s*auto/);
  assert.doesNotMatch(styles, /outline:\s*none|overflow:\s*hidden|white-space:\s*nowrap|\[role\s*=\s*status\]/);
  assert.match(markup, /<header[^>]*>[\s\S]*<nav[\s\S]*<\/nav>[\s\S]*<\/header>/, 'shared body > nav link-hiding rule must not hide the catalog link');
});

// Deliberately small DOM/event doubles. They run the actual host script, shared
// input bridge and shared player binder; browser layout/default focus is not mocked as proof.
function fixture(t) {
  const observers = new Set(), pending = new Set(), elements = [], calls = [];
  class Target {
    listeners = new Map();
    addEventListener(type, callback) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(callback);
    }
    dispatchEvent(event) { for (const callback of this.listeners.get(event.type) ?? []) callback(event); }
  }
  const doc = new Target(), win = new Target();
  doc.hasFocus = () => true;
  doc.hidden = false;
  class Element extends Target {
    constructor(tag, attrs = {}) {
      super();
      this.tagName = tag;
      this.attrs = new Map(Object.entries(attrs));
      this.dataset = {};
      this.style = {};
      this.children = [];
      this.classList = {toggle() {}};
      elements.push(this);
    }
    getAttribute(name) { return this.attrs.get(name) ?? null; }
    setAttribute(name, value) {
      this.attrs.set(name, String(value));
      for (const observer of observers) if (observer.target === this && observer.options.attributeFilter.includes(name)) pending.add(observer);
    }
    get value() { return this._value ?? this.children[0]?.value ?? ''; }
    set value(value) { this._value = String(value); }
    append(...nodes) { this.children.push(...nodes); }
    insertBefore(node, before) {
      const index = this.children.indexOf(before);
      assert.notEqual(index, -1);
      this.children.splice(index, 0, node);
    }
    focus() { doc.activeElement = this; }
    closest(selector) { return selector.split(',').includes(this.tagName) ? this : null; }
  }
  doc.createElement = tag => new Element(tag);
  doc.body = new Element('body');
  doc.head = new Element('head');
  const canvas = new Element('canvas', canvasTag.attrs), controls = new Element('button', controlsTag.attrs);
  doc.querySelector = selector => {
    if (selector === '#lab-canvas') return canvas;
    if (selector === '[data-controls]') return controls;
    assert.fail(`Unexpected host selector: ${selector}`);
  };
  class Observer {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { this.target = target; this.options = options; observers.add(this); }
  }
  const storage = {getItem: () => null, setItem() { assert.fail('host must not change saved bindings'); }};
  const media = Object.assign(new Target(), {matches: false});
  const globals = {document: doc, window: win, navigator: {}, localStorage: storage, matchMedia: () => media};
  for (const [key, value] of Object.entries(globals)) {
    const before = Object.getOwnPropertyDescriptor(globalThis, key);
    Object.defineProperty(globalThis, key, {value, configurable: true});
    t.after(() => { if (before) Object.defineProperty(globalThis, key, before); else delete globalThis[key]; });
  }
  vm.runInNewContext(hostScript, {document: doc, MutationObserver: Observer}, {filename: hostURL.pathname});
  const wasm = {ferravex_key: (...args) => calls.push(args)};
  const input = bindInput(canvas, 'physics-3d', () => wasm, () => true, {
    window: win, document: doc, navigator: {}, storage: () => storage, atob, btoa,
  });
  function key(type, target, code, extras = {}) {
    const event = {type, target, code, key: code, ...extras, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; }};
    target.dispatchEvent(event);
    if (!event.stopped) doc.dispatchEvent(event);
    if (!event.stopped) win.dispatchEvent(event);
    return event;
  }
  function flush() {
    let batches = 0;
    while (pending.size) {
      assert.ok(++batches <= 3, 'description repair must settle without an observer loop');
      const batch = [...pending];
      pending.clear();
      for (const observer of batch) observer.callback();
    }
  }
  return {canvas, controls, doc, wasm, input, calls, key, flush, elements, Element,
    bindPlayer: () => bindPlayer(canvas, 'physics-3d', () => wasm, new Element('div'), () => false)};
}

test('real player initialization retains lab description and accessible title without losing shared help', t => {
  const h = fixture(t);
  h.bindPlayer();
  h.flush();
  assert.equal(h.canvas.getAttribute('aria-labelledby'), 'lab-title');
  const ids = h.canvas.getAttribute('aria-describedby').split(' ');
  assert.deepEqual(new Set(ids), new Set(['ferravex-canvas-help', 'lab-help', 'lab-navigation']));
  assert.equal(ids.length, 3);
  assert.ok(h.elements.some(element => element.id === 'ferravex-canvas-help'));
  h.canvas.setAttribute('aria-describedby', 'future-help future-help');
  h.flush();
  assert.equal(h.canvas.getAttribute('aria-describedby'), 'future-help lab-help lab-navigation');
  h.canvas.setAttribute('aria-describedby', '');
  h.flush();
  assert.equal(h.canvas.getAttribute('aria-describedby'), 'lab-help lab-navigation');
});

test('Tab escapes the game, page keys remain native and key releases still reach the real bridge', t => {
  const h = fixture(t);
  for (const shiftKey of [false, true]) {
    const event = h.key('keydown', h.canvas, 'Tab', {shiftKey});
    assert.equal(event.defaultPrevented, false, 'browser owns normal tab movement');
  }
  assert.deepEqual(h.calls, []);
  for (const tag of ['a', 'summary', 'button', 'input', 'select', 'textarea']) {
    const target = new h.Element(tag);
    for (const code of ['Enter', 'Space', 'KeyR', 'ArrowDown', 'Tab']) {
      assert.equal(h.key('keydown', target, code).defaultPrevented, false, `${tag}: ${code}`);
    }
  }
  assert.deepEqual(h.calls, [], 'page navigation must not drop, reset or pause the lab');
  assert.equal(h.key('keydown', h.canvas, 'KeyR').defaultPrevented, true);
  h.key('keyup', new h.Element('summary'), 'KeyR');
  assert.deepEqual(h.calls, [[114, 1], [114, 0]], 'held game key releases after DOM focus moves');
});

test('Controls navigation, remapped keys, Escape and Details dialog ownership still reach their owners', t => {
  const h = fixture(t);
  h.controls.setAttribute('aria-pressed', 'true');
  for (const code of ['Tab', 'ArrowDown', 'Enter', 'Escape', 'F2', 'KeyZ']) {
    assert.equal(h.key('keydown', h.canvas, code).defaultPrevented, true, code);
    h.key('keyup', h.canvas, code);
  }
  assert.deepEqual(h.calls, [0xff09, 0xff54, 0xff0d, 0xff1b, 0xffbf, 122].flatMap(code => [[code, 1], [code, 0]]));
  assert.equal(h.key('keydown', h.canvas, 'Tab', {shiftKey: true}).defaultPrevented, true, 'retain editor reverse navigation');
  h.controls.setAttribute('aria-pressed', 'false');
  assert.equal(h.key('keydown', h.canvas, 'Tab').defaultPrevented, false, 'page traversal returns once Controls closes');
  const dialog = new h.Element('dialog');
  let delivered = false;
  dialog.addEventListener('keydown', event => { delivered = true; event.stopPropagation(); });
  const before = h.calls.length;
  assert.equal(h.key('keydown', dialog, 'Escape').defaultPrevented, false, 'native dialog cancellation remains available');
  assert.equal(delivered, true);
  assert.equal(h.calls.length, before, 'dialog Escape must not also reach gameplay');
});
