// Actual host sources with a first-party DOM event-path fixture. No generated
// artifacts, browser layout/focus traversal, GPU, or real game Wasm are exercised.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {createContext, runInContext} from 'node:vm';
import {bindInput} from '../input.js';
import {bindGraphicsLifecycle} from '../lifecycle.js';
import {bindVisualPreferences, configureVisuals} from '../visuals.js';

const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const script = path => read(path).replace(/^import .*;$/gm, '').replace(/^export /gm, '');
const runtime = script('../runtime.js');
const boundary = read('../marble-run/index.html').match(/<script id="marble-keyboard-boundary">([\s\S]*?)<\/script>/)[1];
const flush = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return {promise, resolve, reject};
};

class Target {
  listeners = new Map();
  parent = null;
  addEventListener(type, callback, options = false) {
    const capture = typeof options === 'boolean' ? options : !!options.capture;
    const list = this.listeners.get(type) ?? [];
    if (!list.some(row => row.callback === callback && row.capture === capture)) {
      list.push({callback, capture, once: !!options?.once});
    }
    this.listeners.set(type, list);
  }
  removeEventListener(type, callback, options = false) {
    const capture = typeof options === 'boolean' ? options : !!options.capture;
    const list = this.listeners.get(type) ?? [];
    const index = list.findIndex(row => row.callback === callback && row.capture === capture);
    if (index !== -1) list.splice(index, 1);
  }
  count(type) { return this.listeners.get(type)?.length ?? 0; }
}

function emit(target, type, fields = {}) {
  const path = [];
  for (let node = target; node; node = node.parent === node ? null : node.parent) path.push(node);
  const event = {type, target, currentTarget: null, eventPhase: 0, bubbles: true,
    cancelable: true, defaultPrevented: false, stopped: false, immediate: false,
    ...fields,
    preventDefault() { if (this.cancelable) this.defaultPrevented = true; },
    stopPropagation() { this.stopped = true; },
    stopImmediatePropagation() { this.stopped = this.immediate = true; },
  };
  const invoke = (node, capture, phase) => {
    event.currentTarget = node; event.eventPhase = phase;
    for (const row of [...(node.listeners.get(type) ?? [])]) {
      if (row.capture !== capture || !node.listeners.get(type)?.includes(row)) continue;
      if (row.once) node.removeEventListener(type, row.callback, capture);
      row.callback(event);
      if (event.immediate) break;
    }
  };
  for (const node of path.slice(1).reverse()) {
    invoke(node, true, 1);
    if (event.stopped) return event;
  }
  invoke(target, true, 2);
  if (!event.immediate) invoke(target, false, 2);
  if (event.bubbles && !event.stopped) for (const node of path.slice(1)) {
    invoke(node, false, 3);
    if (event.stopped) break;
  }
  event.currentTarget = null; event.eventPhase = 0;
  return event;
}

class Element extends Target {
  constructor(doc, tag) {
    super(); this.ownerDocument = doc; this.tagName = tag; this.children = [];
    this.dataset = {}; this.style = {}; this.attributes = new Map(); this.classes = new Set();
    this.focusCalls = []; this.disabled = false; this.hidden = false;
    this.classList = {
      add: name => this.classes.add(name), remove: name => this.classes.delete(name),
      contains: name => this.classes.has(name),
      toggle: (name, active) => active ? this.classes.add(name) : this.classes.delete(name),
    };
  }
  get isConnected() { return this === this.ownerDocument.documentElement || !!this.parent?.isConnected; }
  get parentElement() { return this.parent instanceof Element ? this.parent : null; }
  set className(value) { this.classes = new Set(value.split(' ')); }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  append(...nodes) { for (const node of nodes) { node.remove(); node.parent = this; this.children.push(node); } }
  remove() {
    if (!this.parentElement) return;
    this.parent.children.splice(this.parent.children.indexOf(this), 1); this.parent = null;
    if (this.ownerDocument.activeElement === this) this.ownerDocument.activeElement = this.ownerDocument.body;
  }
  insertAdjacentElement(position, node) {
    assert.equal(position, 'beforebegin'); node.parent = this.parent;
    this.parent.children.splice(this.parent.children.indexOf(this), 0, node);
  }
  matches(selector) {
    return selector.split(',').some(part => {
      if (part.startsWith('.')) return this.classes.has(part.slice(1));
      if (part.startsWith('#')) return this.id === part.slice(1);
      if (part === '[hidden]') return this.hidden;
      if (part === '[inert]') return this.inert;
      if (part === 'dialog[open]') return this.tagName === 'dialog' && this.open;
      const attr = part.match(/^\[([^=\]]+)(?:=([^\]]+))?\]$/);
      return attr ? this.attributes.has(attr[1]) && (attr[2] === undefined || this.getAttribute(attr[1]) === attr[2]) : this.tagName === part;
    });
  }
  closest(selector) { for (let node = this; node instanceof Element; node = node.parent) if (node.matches(selector)) return node; return null; }
  focus(options) {
    if (!this.isConnected || this.disabled || this.closest('[hidden],[inert]')) return;
    this.focusCalls.push(options);
    const doc = this.ownerDocument, previous = doc.activeElement;
    if (previous === this) return;
    emit(previous, 'blur', {bubbles: false, relatedTarget: this});
    emit(previous, 'focusout', {relatedTarget: this});
    doc.activeElement = this;
    emit(this, 'focus', {bubbles: false, relatedTarget: previous});
    emit(this, 'focusin', {relatedTarget: previous});
  }
  showModal() { this.open = true; }
  close() { this.open = false; }
  getBoundingClientRect() { return {left: 0, top: 0, width: 640, height: 480}; }
}

function fixture({initial = 'body', hidden = false, focused = true, beforeBind = () => {}} = {}) {
  const h = {calls: [], failures: [], timers: new Map(), pending: new Map(), now: 1000, controlsOpen: false};
  const win = h.win = new Target(), doc = h.doc = new Target();
  doc.parent = win; doc.defaultView = win; doc.hidden = hidden; h.focused = focused;
  doc.hasFocus = () => h.focused;
  doc.createElement = tag => new Element(doc, tag);
  doc.documentElement = doc.createElement('html'); doc.documentElement.parent = doc;
  doc.head = doc.createElement('head'); doc.body = doc.createElement('body');
  doc.documentElement.append(doc.head, doc.body); doc.activeElement = doc.body;
  const all = () => { const nodes = []; const visit = node => { nodes.push(node); node.children.forEach(visit); }; visit(doc.documentElement); return nodes; };
  doc.querySelector = selector => all().find(node => node.matches(selector)) ?? null;
  doc.getElementById = id => all().find(node => node.id === id) ?? null;
  for (const [name, tag] of [['controls', 'button'], ['help', 'summary'], ['link', 'a'], ['canvas', 'canvas'], ['status', 'p']]) {
    h[name] = doc.createElement(tag); doc.body.append(h[name]);
  }
  const help = doc.createElement('details'); doc.body.append(help); help.append(h.help);
  h.canvas.id = 'marble-canvas'; h.canvas.dataset.game = 'marble-run';
  h.controls.setAttribute('data-controls', ''); h.controls.disabled = true;
  h.status.setAttribute('role', 'status');
  if (initial !== 'body') h[initial].focus();
  win.parent = win; win.innerWidth = 800; win.innerHeight = 600;
  win.matchMedia = () => ({matches: false});
  win.MutationObserver = class { observe() {} disconnect() {} };
  doc.pointerLockElement = null;
  doc.exitPointerLock = () => { doc.pointerLockElement = null; emit(doc, 'pointerlockchange'); };
  const gl = new Proxy({}, {get: (_, key) => {
    if (String(key).toUpperCase() === key) return 1;
    if (key === 'getParameter') return () => 8192;
    if (key === 'getExtension') return () => null;
    if (['getShaderParameter', 'getProgramParameter'].includes(key)) return () => true;
    return () => ({});
  }});
  h.canvas.getContext = () => gl;
  const engine = h.engine = {
    memory: {buffer: new ArrayBuffer(1024)},
    ferravex_init: () => { h.calls.push(['init']); h.imports.env.ferravex_set_playing(1); },
    ferravex_autoplay() {}, ferravex_visual_quality: () => 2, ferravex_visual_look: () => 0,
    ferravex_feedback_preferences: (...args) => { h.calls.push(['preferences', ...args]); return 1; },
    ferravex_controls_open: () => Number(h.controlsOpen),
    ferravex_input_settings: () => { h.controlsOpen = true; },
    ferravex_frame: () => { h.calls.push(['frame']); return 1; },
    ferravex_details_get: () => 0, ferravex_details_open() {}, ferravex_resolution: () => 1,
    ferravex_vertex_count: () => 0, ferravex_vertices: () => 0, ferravex_batch_count: () => 0, ferravex_batches: () => 0,
    ferravex_command_count: () => 0, ferravex_commands: () => 0, ferravex_clear: () => 0,
    ferravex_culled_count: () => 0, ferravex_visible_count: () => 0,
  };
  for (const name of ['key', 'blur', 'focus']) engine['ferravex_' + name] = (...args) => h.calls.push([name, ...args]);
  const download = h.download = deferred(); let sequence = 0;
  const sandbox = {
    document: doc, window: win, location: {search: '', origin: 'https://local.test'},
    MutationObserver: win.MutationObserver, screen: {orientation: {unlock() {}}},
    matchMedia: win.matchMedia, URLSearchParams, devicePixelRatio: 1, performance,
    console: {error: error => h.failures.push(error.message)},
    setTimeout: callback => { const id = ++sequence; h.timers.set(id, callback); return id; },
    clearTimeout: id => h.timers.delete(id),
    configureVisuals, bindVisualPreferences,
    bindPlayer: () => ({presentationPreferences: {reducedMotion: 'system', reducedFlash: false}, shouldRender: () => true, frame() {}, dispose() { h.calls.push(['dispose-player']); }}),
    bindAudio: () => ({sync() {}, suspend() {}, resume() {}, dispose() { h.calls.push(['dispose-audio']); }}),
    bindInput: (...args) => (h.input = bindInput(...args, {window: win, document: doc, navigator: {}, now: () => h.now})),
    bindGraphicsLifecycle: (canvas, options) => (h.graphics = bindGraphicsLifecycle(canvas, {...options,
      request: callback => { const id = ++sequence; h.pending.set(id, callback); return id; },
      cancel: id => h.pending.delete(id),
    })),
    loadWasm: (url, imports) => { assert.equal(url, 'marble_run.wasm'); h.imports = imports; return download.promise; },
    showLoadError: error => h.failures.push(error.message),
    RetainedRenderer: class { constructor() { this.environment = {quality: 2}; this.meshes = new Map(); } begin() {} renderShadow() {} },
    SpriteRenderer: class { begin() {} }, GpuTimer: class { begin() {} end() {} },
  };
  const context = createContext(sandbox);
  runInContext(script('../fullscreen.js'), context);
  const bindFullscreen = context.bindFullscreen;
  context.bindFullscreen = (...args) => (h.session = bindFullscreen(...args));
  runInContext(script('../details.js'), context);
  runInContext(boundary, context); // Host listeners exist before its module runs.
  beforeBind(h);
  h.boot = runInContext(`(async()=>{${runtime}\n})()`, context);
  h.finish = async () => { download.resolve({instance: {exports: engine}}); await h.boot; assert.deepEqual(h.failures, []); };
  h.key = (type, code, target = doc.activeElement, extra = {}) => emit(target, type, {code, key: code === 'KeyJ' ? 'j' : code, ...extra});
  h.button = doc.querySelector('.ferravex-fullscreen-trigger');
  h.enter = async () => { h.button.focus(); emit(h.button, 'click'); await flush(); assert.equal(h.session.mode, 'expanded'); assert.ok(doc.activeElement === h.canvas, "expected focus on " + h.canvas.tagName); };
  h.tick = () => { assert.equal(h.pending.size, 1); const [id, callback] = h.pending.entries().next().value; h.pending.delete(id); callback(h.now += 16); };
  h.close = () => { emit(win, 'pagehide', {persisted: false}); h.session.dispose(); };
  return h;
}

test('event fixture honors phases independently of registration and distinguishes stop/default cancellation', () => {
  const win = new Target(), doc = new Target(), target = new Target(), seen = [];
  target.parent = doc; doc.parent = win;
  win.addEventListener('keydown', () => seen.push('window bubble'));
  win.addEventListener('keydown', () => seen.push('window capture'), true);
  doc.addEventListener('keydown', event => { seen.push('document first'); event.preventDefault(); event.stopPropagation(); });
  doc.addEventListener('keydown', event => seen.push(`document second ${event.defaultPrevented}`));
  target.addEventListener('keydown', () => seen.push('target'));
  assert.equal(emit(target, 'keydown').defaultPrevented, true);
  assert.deepEqual(seen, ['window capture', 'target', 'document first', 'document second true']);
  seen.length = 0;
  win.addEventListener('keyup', event => { seen.push('capture first'); event.stopImmediatePropagation(); }, true);
  win.addEventListener('keyup', () => seen.push('capture second'), true);
  assert.equal(emit(target, 'keyup').defaultPrevented, false);
  assert.deepEqual(seen, ['capture first']);
});

test('fresh focused boot onboards keyboard play without scrolling and retires its focus observer', async t => {
  const h = fixture(); t.after(h.close);
  assert.equal(h.doc.count('focusin'), 1);
  await h.finish();
  assert.ok(h.doc.activeElement === h.canvas, "expected focus on " + h.canvas.tagName);
  assert.equal(h.canvas.focusCalls.length, 1);
  assert.equal(h.canvas.focusCalls[0]?.preventScroll, true);
  assert.equal(h.doc.count('focusin'), 0);
  h.help.focus(); h.tick(); assert.ok(h.doc.activeElement === h.help, "expected focus on " + h.help.tagName);
  assert.ok(h.calls.some(row => row[0] === 'frame'), 'help focus does not gate authoritative frame work');
});

for (const target of ['help', 'link']) test(`async boot preserves ${target} focus and native keyboard defaults`, async t => {
  const h = fixture(); t.after(h.close); h[target].focus(); await h.finish();
  assert.ok(h.doc.activeElement === h[target], "expected focus on " + h[target].tagName); assert.equal(h.canvas.focusCalls.length, 0);
  assert.equal(h.key('keydown', 'Space').defaultPrevented, false);
  assert.equal(h.key('keyup', 'Space').defaultPrevented, false);
  assert.equal(h.doc.count('focusin'), 0);
});

test('focus moved during download stays relinquished after the chosen control disappears', async t => {
  const h = fixture(); t.after(h.close); h.help.focus(); h.help.remove();
  assert.ok(h.doc.activeElement === h.doc.body, "expected focus on " + h.doc.body.tagName); await h.finish();
  assert.ok(h.doc.activeElement === h.doc.body, "expected focus on " + h.doc.body.tagName); assert.equal(h.canvas.focusCalls.length, 0);
});

test('preexisting control/canvas focus is respected without another focus call', async t => {
  for (const initial of ['help', 'canvas']) {
    const h = fixture({initial}); t.after(h.close); const count = h.canvas.focusCalls.length;
    await h.finish(); assert.ok(h.doc.activeElement === h[initial], "expected focus on " + h[initial].tagName); assert.equal(h.canvas.focusCalls.length, count);
  }
});

test('hidden, unfocused, departed, or disconnected boot cannot claim keyboard focus later', async t => {
  for (const transition of ['hidden', 'blur', 'pagehide', 'removed']) {
    const h = fixture(); t.after(h.close);
    if (transition === 'hidden') { h.doc.hidden = true; emit(h.doc, 'visibilitychange'); h.doc.hidden = false; emit(h.doc, 'visibilitychange'); }
    if (transition === 'blur') { h.focused = false; emit(h.win, 'blur', {bubbles: false}); h.focused = true; emit(h.win, 'focus', {bubbles: false}); }
    if (transition === 'pagehide') { emit(h.win, 'pagehide', {persisted: true}); emit(h.win, 'pageshow', {persisted: true}); }
    if (transition === 'removed') h.canvas.remove();
    await h.finish(); assert.equal(h.canvas.focusCalls.length, 0, transition); assert.equal(h.doc.count('focusin'), 0);
  }
  for (const options of [{hidden: true}, {focused: false}]) {
    const h = fixture(options); t.after(h.close);
    h.doc.hidden = false; h.focused = true; emit(h.win, 'focus', {bubbles: false});
    await h.finish(); assert.equal(h.canvas.focusCalls.length, 0);
  }
});

test('load failure and permanent page teardown release boot focus listeners; late success stays unfocused', async t => {
  const failed = fixture(); t.after(failed.close);
  failed.download.reject(Error('download failed')); await failed.boot;
  assert.ok(failed.failures.includes('download failed')); assert.equal(failed.doc.count('focusin'), 0);
  assert.equal(failed.canvas.focusCalls.length, 0); assert.equal(failed.pending.size, 0);
  const late = fixture(); t.after(late.close); emit(late.win, 'pagehide', {persisted: false});
  assert.equal(late.doc.count('focusin'), 0); await late.finish();
  assert.equal(late.canvas.focusCalls.length, 0); assert.equal(late.pending.size, 0);
  assert.equal(late.calls.filter(row => row[0] === 'dispose-player').length, 1);
});

test('fallback Escape runs in window capture before earlier-registered input bubble', async t => {
  const h = fixture(); t.after(h.close); await h.finish(); await h.enter();
  assert.deepEqual(h.win.listeners.get('keydown').map(row => row.capture), [false, true]);
  let atCapture, targetReached = false;
  h.win.addEventListener('keydown', event => { atCapture = event.defaultPrevented; }, true);
  h.canvas.addEventListener('keydown', () => { targetReached = true; });
  h.calls.length = 0;
  const event = h.key('keydown', 'Escape'); await flush();
  assert.equal(event.defaultPrevented, true); assert.equal(event.immediate, true);
  assert.equal(atCapture, undefined, 'stopImmediatePropagation prevents the later capture listener too');
  assert.equal(targetReached, false); assert.equal(h.session.mode, 'inline');
  assert.ok(h.doc.activeElement === h.button, "expected focus on " + h.button.tagName);
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), [], 'no gameplay Escape down reaches Wasm');
  assert.equal(h.key('keyup', 'Escape').defaultPrevented, false, 'unowned release on restored button retains its default');
});

test('Controls owns Escape in expanded mode before input.syncPanel, then fallback exits once Controls closes', async t => {
  const h = fixture(); t.after(h.close); await h.finish(); await h.enter();
  h.controls.focus(); emit(h.controls, 'click'); assert.ok(h.doc.activeElement === h.canvas, "expected focus on " + h.canvas.tagName);
  assert.equal(h.controlsOpen, true); h.calls.length = 0;
  h.key('keydown', 'Escape'); h.key('keyup', 'Escape'); await flush();
  assert.equal(h.session.mode, 'expanded');
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), [['key', 0xff1b, 1], ['key', 0xff1b, 0]]);
  h.controlsOpen = false; h.key('keydown', 'Escape'); await flush(); assert.equal(h.session.mode, 'inline');
});

test('an earlier capture cancellation remains visible to fullscreen without pretending it stops bubbling', async t => {
  const h = fixture({beforeBind: host => host.win.addEventListener('keydown', event => {
    if (event.key === 'Escape') event.preventDefault();
  }, true)});
  t.after(h.close); await h.finish(); await h.enter(); h.calls.length = 0;
  let observed;
  h.win.addEventListener('keydown', event => { observed = event.defaultPrevented; }, true);
  assert.equal(h.key('keydown', 'Escape').defaultPrevented, true); await flush();
  assert.equal(observed, true); assert.equal(h.session.mode, 'expanded');
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), [['key', 0xff1b, 1]], 'preventDefault alone does not stop the real input listener');
});

test('actual Details dialog keeps Escape through capture and closes through its cancel owner', async t => {
  const h = fixture(); t.after(h.close); await h.finish(); h.tick(); await h.enter();
  const details = h.doc.querySelector('.ferravex-details-trigger'); details.focus(); emit(details, 'click');
  const dialog = h.doc.querySelector('dialog[open]'); assert.ok(dialog); h.calls.length = 0;
  const event = h.key('keydown', 'Escape'); assert.equal(event.defaultPrevented, false);
  assert.equal(event.stopped, true); assert.equal(h.session.mode, 'expanded');
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), []);
  // The browser's default Escape action dispatches cancel; this fixture invokes
  // that separate event explicitly, executing the real Details cancel listener.
  assert.equal(emit(dialog, 'cancel', {bubbles: false}).defaultPrevented, true);
  assert.equal(dialog.open, false); assert.ok(h.doc.activeElement === h.canvas, "expected focus on " + h.canvas.tagName);
  assert.equal(h.session.mode, 'expanded');
});

test('reserved host exit, remapped gameplay keys and held releases survive the combined listeners', async t => {
  const h = fixture(); t.after(h.close); await h.finish(); await h.enter(); h.calls.length = 0;
  assert.equal(h.key('keydown', 'Tab', h.canvas, {shiftKey: true}).defaultPrevented, true, 'fullscreen routes focus to its visible game menu');
  assert.equal(h.key('keyup', 'Tab', h.help, {shiftKey: true}).defaultPrevented, false);
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), []);
  for (const [code, value] of [['Tab', 0xff09], ['F2', 0xffbf], ['KeyJ', 106], ['Semicolon', 59]]) {
    h.canvas.focus(); assert.equal(h.key('keydown', code).defaultPrevented, true);
    h.help.focus(); assert.equal(h.key('keyup', code).defaultPrevented, true);
    assert.deepEqual(h.calls.slice(-2), [['key', value, 1], ['key', value, 0]]);
  }
  assert.equal(h.session.mode, 'expanded');
  for (const modifier of ['ctrlKey', 'metaKey', 'altKey']) {
    h.canvas.focus(); const before = h.calls.length;
    assert.equal(h.key('keydown', 'KeyJ', h.canvas, {[modifier]: true}).defaultPrevented, false);
    assert.equal(h.calls.length, before);
  }
});

test('native fullscreen leaves Escape to browser/input and pointer-unlock deduplication remains intact', async t => {
  const h = fixture(); t.after(h.close); await h.finish();
  h.doc.documentElement.requestFullscreen = () => { h.doc.fullscreenElement = h.doc.documentElement; emit(h.doc, 'fullscreenchange'); };
  h.button.focus(); emit(h.button, 'click'); await flush(); assert.equal(h.session.mode, 'native');
  h.doc.pointerLockElement = h.canvas; emit(h.doc, 'pointerlockchange'); h.calls.length = 0;
  h.key('keydown', 'Escape'); h.key('keyup', 'Escape'); h.doc.exitPointerLock();
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), [['key', 0xff1b, 1], ['key', 0xff1b, 0]]);
  h.doc.fullscreenElement = null; emit(h.doc, 'fullscreenchange');
  assert.equal(h.session.mode, 'inline'); assert.ok(h.doc.activeElement === h.button, "expected focus on " + h.button.tagName);
  h.now += 300; h.doc.pointerLockElement = h.canvas; emit(h.doc, 'pointerlockchange'); h.calls.length = 0;
  h.controlsOpen = true; h.input.syncPanel();
  assert.deepEqual(h.calls.filter(row => row[0] === 'key'), [], 'intentional Controls unlock never synthesizes an Escape');
});
