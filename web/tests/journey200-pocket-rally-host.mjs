// Source/DOM contracts only: no dist artifacts, Wasm compilation or browser playtest.
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {test} from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';
import {bindDetails} from '../details.js';

const pageURL = new URL('../pocket-rally/index.html', import.meta.url);
const html = readFileSync(pageURL, 'utf8');
const attributes = text => Object.fromEntries([...text.matchAll(/([\w-]+)(?:="([^"]*)")?/g)].map(([, key, value]) => [key, value ?? '']));
const tags = [...html.matchAll(/<([a-z][\w-]*)\b([^>]*)>/g)].map(([, tag, attrs]) => ({tag, ...attributes(attrs)}));
const byId = id => { const node = tags.find(node => node.id === id); assert.ok(node, id); return node; };
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
const hostScript = scripts.find(([, attrs]) => !attributes(attrs).src)?.[2];
const text = html.replace(/<script\b[^>]*>[\s\S]*?<\/script>|<style>[\s\S]*?<\/style>/g, '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ');

test('canvas name, description, fallback and live loading status are game-specific', () => {
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>Pocket Rally · Ferravex<\/title>/);
  assert.equal(tags.filter(node => node.tag === 'canvas').length, 1);
  const canvas = byId('rally-canvas');
  assert.equal(canvas.tabindex, '0');
  assert.equal(canvas['data-game'], 'pocket-rally');
  assert.equal(canvas['aria-labelledby'], 'rally-title');
  assert.equal(byId(canvas['aria-labelledby']).tag, 'h1');
  for (const id of canvas['aria-describedby'].split(' ')) assert.equal(byId(id).tag, 'p');
  const ids = tags.filter(node => node.id).map(node => node.id);
  assert.equal(new Set(ids).size, ids.length, 'description references must be unambiguous');
  assert.match(text, /sixteen marked gates in order on each of three laps/);
  assert.match(text, /race starts automatically with a 3, 2, 1, GO countdown/);
  assert.match(text, /JavaScript and WebGL2 are required to play/);
  const status = byId('rally-status');
  assert.equal(status.role, 'status');
  assert.equal(status['aria-live'], 'polite');
  assert.equal(status['aria-atomic'], 'true');
  assert.equal(tags.filter(node => node.role === 'status').length, 1, 'runtime selects the loading status before creating player tools');
  assert.ok(!tags.some(node => node.role === 'application'), 'page navigation remains available');
});

test('defaults and menu guidance defer to saved controls and do not promise touch gestures', () => {
  assert.match(text, /Default keyboard controls/);
  assert.match(text, /Saved bindings may differ/);
  assert.match(text, /view or change your current keyboard and gamepad bindings/);
  for (const keys of [/A \/ D/, /W .*Up arrow/, /S .*Down arrow/, /Handbrake Space/, /Pause \/ resume Escape/, /Restart race R/]) assert.match(text, keys);
  assert.match(text, /after the finish, Enter also starts another race/);
  assert.match(text, /Escape cancels a pending rebind first/);
  assert.match(text, /Done or Escape closes Details and returns focus to the canvas/);
  assert.match(text, /opening the guide does not pause the race/);
  assert.doesNotMatch(text, /swipe|tap to|touch controls|touchscreen/i);
  for (const hook of ['data-controls', 'data-audio']) {
    const buttons = tags.filter(node => Object.hasOwn(node, hook));
    assert.equal(buttons.length, 1);
    assert.equal(buttons[0].type, 'button');
    assert.ok(Object.hasOwn(buttons[0], 'disabled'), 'shared runtime owns readiness');
    assert.equal(buttons[0]['aria-pressed'], 'false');
  }
  assert.ok(!tags.some(node => /ferravex-(details|fullscreen)-trigger/.test(node.class ?? '')), 'shared player buttons are not duplicated');
});

test('catalog, nested site routes and the existing runtime/Wasm boundary stay intact', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const game = catalog.find(game => game.id === 'pocket-rally');
  assert.equal(game.host, 'pocket-rally');
  assert.equal(game.title, 'Pocket Rally');
  const boot = scripts.filter(([, attrs]) => attributes(attrs).src);
  assert.equal(boot.length, 1);
  assert.equal(attributes(boot[0][1]).type, 'module');
  assert.equal(attributes(boot[0][1]).src, '../runtime.js');
  assert.ok(existsSync(new URL('../runtime.js', pageURL)));
  assert.ok(existsSync(new URL('../index.html', pageURL)));
  for (const prefix of ['/', '/play/']) {
    const route = new URL(`${prefix}${game.host}/`, 'https://ferravex.test');
    assert.equal(new URL(tags.find(node => node.tag === 'a').href, route).pathname, prefix);
    assert.equal(new URL(attributes(boot[0][1]).src, route).pathname, `${prefix}runtime.js`);
    assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, route).pathname, `${prefix}pocket-rally/pocket_rally.wasm`);
  }
  const runtime = readFileSync(new URL('../runtime.js', import.meta.url), 'utf8');
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-',\s*'_'\)\}\.wasm`,imports\)/);
  assert.match(runtime, /exports\.ferravex_init\(\)/);
  assert.match(runtime, /exports\.ferravex_frame\(dt,/);
  assert.match(runtime, /bindDetails\(canvas,game,/);
  assert.match(runtime, /bindGraphicsLifecycle\(canvas,/);
  assert.match(runtime, /suspendOnPageLifecycle:true/);
  assert.match(runtime, /inputPlatform\.suspend\(\)/);
  assert.match(runtime, /inputPlatform\.resume\(\)/);
  assert.doesNotMatch(hostScript, /ferravex_|WebAssembly|fetch\(|requestAnimationFrame|setInterval|\.focus\(/, 'host help must not own simulation, loading or focus restoration');
});

test('responsive source keeps toolbar wrapping, focus visible and text readable', () => {
  const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
  assert.match(css, /#pocket-rally-host nav\s*\{[^}]*height: auto;[^}]*flex-wrap: wrap/);
  assert.match(css, /#pocket-rally-host nav button[^}]*min-height: 44px/);
  assert.match(css, /#pocket-rally-host :focus-visible\s*\{[^}]*outline: 3px/);
  assert.match(css, /#rally-canvas:focus-visible\s*\{[^}]*outline-offset: -4px/);
  assert.match(css, /#rally-game\s*\{[^}]*flex: 1 0 324px;[^}]*min-height: 324px/);
  assert.match(css, /#rally-guide\s*\{[^}]*overflow-wrap: anywhere/);
  assert.match(css, /@media \(forced-colors: active\)/);
  assert.doesNotMatch(css, /outline:\s*none|white-space:\s*nowrap|overflow:\s*hidden/);
  assert.doesNotMatch(css, /(?:^|\})\s*\[role[=:]status\]/, 'loading styles must not reposition shared player status messages');
});

// Small browser-API fixture. It executes the actual inline host script and actual
// shared input/Details/player modules. It does not implement layout, native Tab
// traversal, assistive technology or rendering; those need parent browser checks.
function fixture(t) {
  const elements = [], observers = [], pending = new Set(), calls = [];
  class Element {
    constructor(tag) {
      this.tagName = tag; this.children = []; this.attrs = {}; this.listeners = new Map();
      this.dataset = {}; this.style = {}; this.hidden = false; this.disabled = false; this._text = '';
      const classes = new Set();
      this.classList = {add: v => classes.add(v), remove: v => classes.delete(v), contains: v => classes.has(v), toggle(v, on) { if (on) classes.add(v); else classes.delete(v); }};
      elements.push(this);
    }
    addEventListener(type, callback) { if (!this.listeners.has(type)) this.listeners.set(type, new Set()); this.listeners.get(type).add(callback); }
    removeEventListener(type, callback) { this.listeners.get(type)?.delete(callback); }
    dispatchEvent(event) {
      if (!event.target) Object.defineProperty(event, 'target', {value: this});
      for (const callback of this.listeners.get(event.type) ?? []) callback(event);
      if (event.bubbles && !event.cancelBubble) this.parentElement?.dispatchEvent(event);
      return !event.defaultPrevented;
    }
    emit(type, fields = {}) { const event = new Event(type, {bubbles: true, cancelable: true}); Object.assign(event, fields); this.dispatchEvent(event); return event; }
    append(...nodes) { for (const node of nodes) { node.parentElement = this; this.children.push(node); } }
    insertBefore(node, before) { const index = this.children.indexOf(before); assert.ok(index >= 0); node.parentElement = this; this.children.splice(index, 0, node); }
    insertAdjacentElement(where, node) { assert.equal(where, 'beforebegin'); this.parentElement.insertBefore(node, this); }
    replaceChildren(...nodes) { for (const child of this.children) child.parentElement = null; this.children = []; this._text = ''; this.append(...nodes); }
    remove() { if (this.parentElement) this.parentElement.children.splice(this.parentElement.children.indexOf(this), 1); this.parentElement = null; }
    contains(node) { return this === node || this.children.some(child => child.contains(node)); }
    get isConnected() { return doc.contains(this); }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this.replaceChildren(); this._text = String(value); }
    get value() { return this._value ?? (this.tagName === 'select' ? this.children[0]?.value : '') ?? ''; }
    set value(value) { this._value = String(value); }
    setAttribute(key, value) {
      this.attrs[key] = String(value);
      if (key === 'id') this.id = String(value);
      for (const observer of observers) if (observer.target === this && observer.options.attributeFilter?.includes(key)) pending.add(observer.callback);
    }
    getAttribute(key) { return this.attrs[key] ?? null; }
    matches(selector) {
      return selector.split(',').some(part => {
        if (part === 'dialog[open]') return this.tagName === 'dialog' && this.open;
        if (part.startsWith('#')) return this.id === part.slice(1);
        if (part.startsWith('.')) return this.className === part.slice(1);
        if (part.startsWith('[')) return Object.hasOwn(this.attrs, part.slice(1, -1));
        return this.tagName === part;
      });
    }
    closest(selector) { return this.matches(selector) ? this : this.parentElement?.closest(selector) ?? null; }
    querySelector(selector) { return elements.find(node => node !== this && this.contains(node) && node.matches(selector)) ?? null; }
    focus() { doc.activeElement = this; }
    click() { if (!this.disabled) this.emit('click'); }
    showModal() { this.open = true; }
    close() { this.open = false; }
    getBoundingClientRect() { return {left: 0, top: 0, width: 640, height: 360}; }
  }
  const doc = new Element('document'), win = new Element('window');
  win.append(doc); win.parent = win;
  doc.createElement = tag => new Element(tag); doc.hidden = false; doc.hasFocus = () => true;
  doc.documentElement = new Element('html'); doc.head = new Element('head'); doc.body = new Element('body');
  doc.append(doc.documentElement); doc.documentElement.append(doc.head, doc.body);
  doc.getElementById = id => elements.find(node => node.id === id) ?? null;
  const create = info => {
    const element = new Element(info.tag);
    for (const [key, value] of Object.entries(info)) if (key !== 'tag') element.setAttribute(key, value);
    element.hidden = Object.hasOwn(info, 'hidden'); element.disabled = Object.hasOwn(info, 'disabled');
    return element;
  };
  const nav = create(tags.find(node => node.tag === 'nav')), link = create(tags.find(node => node.tag === 'a'));
  const controls = create(tags.find(node => Object.hasOwn(node, 'data-controls')));
  const audio = create(tags.find(node => Object.hasOwn(node, 'data-audio')));
  const canvas = create(byId('rally-canvas')), guide = create(byId('rally-guide')), help = create(byId('rally-help-toggle'));
  doc.body.append(nav, canvas, help, guide); nav.append(link, controls, audio);
  doc.activeElement = canvas;
  const MockObserver = class {
    constructor(callback) { this.callback = callback; observers.push(this); }
    observe(target, options) { this.target = target; this.options = options; }
    disconnect() { this.target = null; }
  };
  const storage = {getItem: () => null, setItem() { assert.fail('host navigation must not write settings'); }};
  const media = new Element('media'); media.matches = false;
  const globals = {document: doc, window: win, navigator: {onLine: true}, localStorage: storage, MutationObserver: MockObserver,
    matchMedia: () => media, location: {search: '', href: 'https://ferravex.test/pocket-rally/'}, innerWidth: 640, innerHeight: 480, devicePixelRatio: 1};
  for (const [name, value] of Object.entries(globals)) {
    const previous = Object.getOwnPropertyDescriptor(globalThis, name);
    Object.defineProperty(globalThis, name, {value, configurable: true, writable: true});
    t.after(() => { if (previous) Object.defineProperty(globalThis, name, previous); else delete globalThis[name]; });
  }
  runInNewContext(hostScript, {document: doc, MutationObserver: MockObserver});
  let ready = false;
  const exports = {ferravex_details_get: () => 0, ferravex_details_set() {}, ferravex_controls_open: () => controls.getAttribute('aria-pressed') === 'true' ? 1 : 0};
  for (const name of ['key', 'blur', 'focus', 'input_settings', 'details_open']) exports[`ferravex_${name}`] = (...args) => calls.push([name, ...args]);
  const input = bindInput(canvas, 'pocket-rally', () => ready ? exports : undefined, () => true,
    {window: win, document: doc, navigator: {}, storage: () => storage, atob, btoa});
  return {doc, win, canvas, nav, link, controls, help, guide, calls, input,
    boot() { ready = true; return bindDetails(canvas, 'pocket-rally', () => exports); },
    flush() { let count = 0; while (pending.size) { assert.ok(++count <= 3, 'description observer must settle'); const callbacks = [...pending]; pending.clear(); for (const callback of callbacks) callback(); } },
    button(label) { const button = elements.find(node => node.tagName === 'button' && node.textContent === label); assert.ok(button, label); return button; },
  };
}

test('help disclosure works before boot without moving focus or changing gameplay', t => {
  const h = fixture(t);
  h.help.focus(); h.help.click();
  assert.equal(h.guide.hidden, false); assert.equal(h.help.getAttribute('aria-expanded'), 'true');
  assert.equal(h.doc.activeElement, h.help);
  h.help.click(); assert.equal(h.guide.hidden, true); assert.equal(h.help.getAttribute('aria-expanded'), 'false');
  assert.deepEqual(h.calls, []);
});

test('shared player setup preserves game description and keeps one set of player tools', t => {
  const h = fixture(t);
  h.boot(); h.flush();
  assert.equal(h.canvas.getAttribute('aria-labelledby'), 'rally-title');
  assert.deepEqual(h.canvas.getAttribute('aria-describedby').split(' '), ['ferravex-canvas-help', 'rally-objective', 'rally-focus', 'rally-bindings']);
  for (const label of ['Details', 'Fullscreen', 'Save screenshot', 'Export player settings']) h.button(label);
  h.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help'); h.flush();
  assert.equal(new Set(h.canvas.getAttribute('aria-describedby').split(' ')).size, 4);
  h.canvas.setAttribute('aria-label', 'Pocket Rally game canvas'); h.flush();
  assert.equal(h.canvas.getAttribute('aria-labelledby'), 'rally-title', 'later shared Game guide cannot replace the title reference');
});

test('Tab can leave the canvas, Controls keeps Tab, and gameplay keys and releases still reach the bridge', t => {
  const h = fixture(t); h.boot();
  assert.equal(h.canvas.emit('keydown', {code: 'Tab'}).defaultPrevented, false);
  assert.deepEqual(h.calls, []);
  h.controls.setAttribute('aria-pressed', 'true');
  assert.equal(h.canvas.emit('keydown', {code: 'Tab'}).defaultPrevented, true);
  assert.deepEqual(h.calls.at(-1), ['key', 0xff09, 1]);
  h.canvas.emit('keyup', {code: 'Tab'}); h.calls.length = 0;
  assert.equal(h.canvas.emit('keydown', {code: 'Tab', shiftKey: true}).defaultPrevented, false);
  assert.deepEqual(h.calls, []);
  for (const [code, value] of [['Escape', 0xff1b], ['F2', 0xffbf], ['KeyR', 114], ['Space', 32]]) {
    h.canvas.emit('keydown', {code}); h.canvas.emit('keyup', {code});
    assert.deepEqual(h.calls.slice(-2), [['key', value, 1], ['key', value, 0]]);
  }
  h.canvas.emit('keydown', {code: 'KeyW'}); h.canvas.emit('keydown', {code: 'Tab', shiftKey: true});
  h.help.focus(); h.help.emit('keyup', {code: 'KeyW'});
  assert.deepEqual(h.calls.at(-1), ['key', 119, 0], 'tabbing to page UI must not strand a held driving key');
});

test('native catalog/help activation stays available and Details Escape restores canvas focus', t => {
  const h = fixture(t), details = h.boot();
  assert.equal(h.link.emit('keydown', {code: 'Enter'}).defaultPrevented, false);
  for (const code of ['Enter', 'Space']) {
    assert.equal(h.help.emit('keydown', {code}).defaultPrevented, false);
    assert.equal(h.help.emit('keyup', {code}).defaultPrevented, false);
  }
  assert.deepEqual(h.calls, []);
  details.ready(); h.button('Details').click();
  assert.equal(details.isOpen(), true); assert.equal(h.doc.activeElement, h.button('Done'));
  const dialog = h.doc.querySelector('dialog[open]');
  h.calls.length = 0; dialog.emit('keydown', {code: 'Escape', key: 'Escape'});
  assert.deepEqual(h.calls, [], 'Details must keep Escape out of gameplay');
  assert.equal(dialog.emit('cancel').defaultPrevented, true);
  assert.equal(details.isOpen(), false); assert.equal(h.doc.activeElement, h.canvas);
  assert.deepEqual(h.calls, [['details_open', 0], ['blur'], ['focus', 1]]);
  h.input.suspend(); const before = h.calls.length;
  h.canvas.emit('keydown', {code: 'KeyW'}); assert.equal(h.calls.length, before);
  h.input.resume(); h.canvas.emit('keydown', {code: 'KeyW'});
  assert.deepEqual(h.calls.at(-1), ['key', 119, 1], 'host additions preserve shared lifecycle input suspension');
});
