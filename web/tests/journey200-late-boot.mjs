// Execute current runtime source with real lifecycle/visual helpers and focused
// DOM, WebGL, Wasm and module-loader doubles. No game artifact or browser runs.
import assert from 'node:assert/strict';
import {getEventListeners} from 'node:events';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindGraphicsLifecycle} from '../lifecycle.js';
import {bindVisualPreferences, configureVisuals} from '../visuals.js';
import {framebufferSize} from '../details.js';

const runtime = readFileSync(new URL('../runtime.js', import.meta.url), 'utf8')
  .replace(/^import .*;$/gm, '').replace(/\bimport\(/g, 'importTool(');
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return {promise, resolve, reject};
}
const flush = () => new Promise(resolve => setImmediate(resolve));

function fixture({search = ''} = {}) {
  const h = {calls: [], failures: [], loadErrors: [], pending: new Map(), modules: new Map(),
    download: deferred(), compileFails: false, now: 0};
  class Node extends EventTarget {
    dataset = {}; style = {}; attributes = new Map();
    width = 640; height = 480; disabled = false; textContent = ''; isConnected = true;
    setAttribute(name, value) { this.attributes.set(name, String(value)); }
    getBoundingClientRect() { return {width: this.width, height: this.height, left: 0, top: 0}; }
    focus(options) { h.calls.push(['focus', options]); doc.activeElement = this; emit(doc, 'focusin'); }
    append() {}
    getContext() { return gl; }
  }
  const emit = (target, type, props = {}) => target.dispatchEvent(Object.assign(new Event(type, {cancelable: true}), props));
  const win = h.win = new Node(), doc = h.doc = new Node(), canvas = h.canvas = new Node();
  doc.defaultView = win; doc.body = new Node(); doc.documentElement = new Node();
  doc.activeElement = doc.body; doc.hidden = false; doc.hasFocus = () => true;
  doc.createElement = () => new Node();
  canvas.ownerDocument = doc; canvas.dataset.game = 'neon-swarm';
  const media = h.media = {matches: false}; win.matchMedia = () => media;
  const controls = h.controls = new Node(), multiplayer = h.multiplayer = new Node();
  controls.disabled = multiplayer.disabled = true;
  const status = h.status = new Node(); status.textContent = 'Loading original Rust engine…';
  const nodes = new Map([['canvas', canvas], ['[role=status]', status], ['[data-controls]', controls], ['[data-multiplayer]', multiplayer]]);
  doc.querySelector = selector => nodes.get(selector) ?? null;
  const gl = new Proxy({}, {get: (_, key) => {
    if (String(key).toUpperCase() === key) return 1;
    if (key === 'getParameter') return () => 8192;
    if (key === 'getExtension') return () => null;
    if (key === 'getShaderParameter') return () => !h.compileFails;
    if (key === 'getShaderInfoLog') return () => 'restoration shader compilation failed';
    if (key === 'getProgramParameter') return () => true;
    return () => ({});
  }});
  const engine = h.engine = {
    memory: {buffer: new ArrayBuffer(1024)},
    ferravex_init() { h.calls.push(['init']); h.onInit?.(); },
    ferravex_autoplay(value) { h.calls.push(['autoplay', value]); },
    ferravex_visual_quality: () => 2, ferravex_visual_look: () => 0,
    ferravex_feedback_preferences(motion, flash) { h.calls.push(['preferences', motion, flash]); return 1; },
    ferravex_frame(dt) { h.calls.push(['frame', dt]); return 1; },
    ferravex_details_get: () => 0, ferravex_resolution: () => 1,
    ferravex_vertex_count: () => 0, ferravex_vertices: () => 0,
    ferravex_batch_count: () => 0, ferravex_batches: () => 0,
    ferravex_command_count: () => 0, ferravex_commands: () => 0,
    ferravex_clear: () => 0, ferravex_culled_count: () => 0, ferravex_visible_count: () => 0,
  };
  const player = {presentationPreferences: {reducedMotion: 'system', reducedFlash: false},
    shouldRender: () => true, frame() {}, dispose() { h.calls.push(['dispose-player']); }};
  let sequence = 0;
  const sandbox = {
    document: doc, window: win, location: {search}, URLSearchParams, devicePixelRatio: 1, performance,
    console: {error: error => h.failures.push(error)},
    bindVisualPreferences: (...args) => (h.visuals = bindVisualPreferences(...args)), configureVisuals, framebufferSize,
    bindDetails: () => ({player, ready() {}, update() {}}),
    bindInput: (target, game, getExports) => {
      h.getExports = getExports;
      return {imports: {}, poll() {}, syncPanel: () => false,
        openSettings() { h.calls.push(['controls']); },
        suspend() { h.calls.push(['suspend-input']); }, resume() { h.calls.push(['resume-input']); }};
    },
    bindAudio: () => ({sync() {}, suspend() { h.calls.push(['suspend-audio']); },
      resume() { h.calls.push(['resume-audio']); }, dispose() { h.calls.push(['dispose-audio']); }}),
    bindGraphicsLifecycle: (target, options) => (h.graphics = bindGraphicsLifecycle(target, {...options,
      request: callback => { const id = sequence++; h.pending.set(id, callback); return id; },
      cancel: id => h.pending.delete(id),
    })),
    loadWasm: (url, imports) => { assert.equal(url, 'neon_swarm.wasm'); h.imports = imports; return h.download.promise; },
    showLoadError(error, target) { h.loadErrors.push(error); target.textContent = error.message; },
    importTool(path) {
      assert.equal(h.modules.has(path), false, 'one import per optional tool');
      const loading = deferred(); h.modules.set(path, loading); return loading.promise;
    },
    RetainedRenderer: class {
      constructor() { this.environment = {quality: 2}; this.meshes = new Map(); }
      begin() {} renderShadow() {}
    },
    SpriteRenderer: class { begin() {} }, GpuTimer: class { begin() {} end() {} },
  };
  h.boot = runInNewContext(`(async()=>{${runtime}\n})()`, sandbox);
  h.emit = emit;
  h.resolveLoad = () => h.download.resolve({instance: {exports: engine}});
  h.finish = async () => { h.resolveLoad(); await h.boot; };
  h.lose = () => emit(canvas, 'webglcontextlost');
  h.restore = () => emit(canvas, 'webglcontextrestored');
  h.failRecovery = () => { h.lose(); h.compileFails = true; h.restore(); };
  h.close = () => emit(win, 'pagehide', {persisted: false});
  h.tick = (dt = 16) => {
    assert.equal(h.pending.size, 1, 'one scheduled frame');
    const [id, callback] = h.pending.entries().next().value; h.pending.delete(id); callback(h.now += dt);
  };
  h.module = kind => ({['create' + kind[0].toUpperCase() + kind.slice(1) + 'Tools']: () => {
    h.calls.push(['create-tool', kind]);
    return {update() {}, toggle() { h.calls.push(['toggle-tool', kind]); },
      dispose() { h.calls.push(['dispose-tool', kind]); }};
  }});
  return h;
}

const count = (h, name) => h.calls.filter(row => row[0] === name).length;
function detached(h) {
  assert.deepEqual(h.graphics.reasons(), ['stopped']);
  assert.equal(h.pending.size, 0);
  assert.equal(count(h, 'dispose-player'), 1);
  assert.equal(count(h, 'dispose-audio'), 1);
  assert.equal(h.visuals.refresh(true), false);
  for (const type of ['resize', 'pagehide', 'pageshow', 'blur']) assert.equal(getEventListeners(h.win, type).length, 0, type);
  assert.equal(getEventListeners(h.doc, 'focusin').length, 0);
  assert.equal(getEventListeners(h.controls, 'click').length, 0);
  assert.equal(getEventListeners(h.multiplayer, 'click').length, 0);
  for (const type of ['webglcontextlost', 'webglcontextrestored']) assert.equal(getEventListeners(h.canvas, type).length, 0, type);
}

for (const outcome of ['success', 'rejection']) test(`failed recovery during download survives late ${outcome}`, async () => {
  const h = fixture();
  h.failRecovery();
  const failure = h.status.textContent;
  assert.match(failure, /^Graphics recovery failed: restoration shader compilation failed$/);
  detached(h);
  if (outcome === 'success') h.resolveLoad();
  else h.download.reject(Error('late download failure'));
  await h.boot;
  assert.equal(count(h, 'init'), 0, 'late load must not initialize a disposed runtime');
  assert.equal(h.getExports(), undefined, 'late instance never becomes the host instance');
  assert.equal(count(h, 'autoplay'), 0);
  assert.equal(count(h, 'preferences'), 0);
  assert.equal(count(h, 'focus'), 0);
  assert.equal(h.status.textContent, failure);
  assert.equal(h.controls.disabled, true);
  assert.equal(h.multiplayer.disabled, true);
  assert.deepEqual(h.loadErrors, []);
  assert.equal(h.failures.length, 1);
  h.close(); h.restore(); h.emit(h.win, 'pageshow');
  detached(h);
});

for (const outcome of ['success', 'rejection']) test(`permanent pagehide during download ignores late ${outcome}`, async () => {
  const h = fixture(); h.close();
  const previous = h.status.textContent;
  if (outcome === 'success') h.resolveLoad();
  else h.download.reject(Error('late download failure'));
  await h.boot;
  assert.equal(count(h, 'init'), 0);
  assert.equal(h.getExports(), undefined);
  assert.equal(h.status.textContent, previous);
  assert.equal(h.controls.disabled, true);
  assert.equal(h.multiplayer.disabled, true);
  assert.deepEqual(h.failures, []);
  assert.deepEqual(h.loadErrors, []);
  detached(h);
});

test('ordinary load rejection remains actionable and cleans up once', async () => {
  const h = fixture(), cause = Error('download failed');
  h.download.reject(cause); await h.boot;
  assert.equal(h.status.textContent, 'download failed');
  assert.deepEqual(h.loadErrors, [cause]); assert.deepEqual(h.failures, [cause]);
  assert.equal(count(h, 'init'), 0); assert.equal(h.controls.disabled, true);
  h.close(); detached(h);
});

test('load failure while temporarily suspended still reports its own actionable error', async () => {
  const h = fixture(); h.lose(); h.emit(h.win, 'pagehide', {persisted: true});
  const cause = Error('compile failed'); h.download.reject(cause); await h.boot;
  assert.equal(h.status.textContent, cause.message); assert.deepEqual(h.loadErrors, [cause]);
  h.restore(); h.emit(h.win, 'pageshow', {persisted: true});
  assert.equal(h.status.textContent, cause.message); detached(h);
});

test('successful recovery before download finishes retains loading status and boots once', async t => {
  const h = fixture(); t.after(h.close);
  const loading = h.status.textContent;
  h.lose(); h.restore();
  assert.equal(h.graphics.active(), true);
  assert.equal(h.status.textContent, loading, 'graphics ready is not game boot complete');
  assert.equal(h.visuals.error, null, 'rebuilding graphics before Wasm exists is not a missing-ABI error');
  assert.equal(count(h, 'init'), 0); assert.equal(h.controls.disabled, true);
  assert.equal(h.pending.size, 0);
  await h.finish();
  assert.equal(count(h, 'init'), 1); assert.equal(count(h, 'autoplay'), 1);
  assert.equal(h.status.textContent, ''); assert.equal(h.controls.disabled, false);
  assert.equal(h.canvas.dataset.visualPreferencesError, '');
  h.tick(); assert.equal(count(h, 'frame'), 1); assert.deepEqual(h.failures, []);
});

test('load completion during transient loss retains suspension until successful restoration', async t => {
  const h = fixture(); t.after(h.close); h.lose();
  const lost = h.status.textContent;
  await h.finish();
  assert.equal(count(h, 'init'), 1, 'temporary loss does not discard a valid loaded instance');
  assert.equal(h.status.textContent, lost); assert.equal(h.pending.size, 0);
  assert.equal(count(h, 'preferences'), 0); assert.equal(count(h, 'focus'), 0);
  h.media.matches = true; h.restore();
  assert.equal(h.status.textContent, ''); assert.equal(h.canvas.dataset.graphicsState, 'ready');
  assert.deepEqual(h.calls.find(row => row[0] === 'preferences'), ['preferences', 1, 0]);
  h.tick(); assert.equal(count(h, 'frame'), 1); assert.equal(count(h, 'init'), 1);
});

test('BFCache and freeze preserve the boot and preference bridge, resuming only after final recovery', async t => {
  const h = fixture(); t.after(h.close);
  h.emit(h.win, 'pagehide', {persisted: true}); h.emit(h.doc, 'freeze'); h.lose();
  const suspended = h.status.textContent;
  await h.finish();
  assert.equal(count(h, 'init'), 1); assert.equal(count(h, 'dispose-player'), 0);
  assert.equal(getEventListeners(h.win, 'resize').length, 1);
  assert.equal(h.status.textContent, suspended); assert.equal(h.pending.size, 0);
  h.emit(h.win, 'pageshow', {persisted: true}); h.restore();
  assert.deepEqual(h.graphics.reasons(), ['frozen']); assert.equal(h.pending.size, 0);
  h.media.matches = true; h.emit(h.doc, 'resume'); h.tick();
  assert.equal(h.visuals.current.reducedMotion, true);
  assert.equal(count(h, 'frame'), 1); assert.equal(count(h, 'resume-input'), 1);
  assert.equal(count(h, 'focus'), 0, 'a BFCache return never reclaims boot focus');
  assert.equal(count(h, 'init'), 1); assert.deepEqual(h.failures, []);
});

test('boot respects a focus choice that is blurred before successful recovery', async t => {
  const h = fixture(); t.after(h.close);
  h.doc.activeElement = h.controls; h.emit(h.doc, 'focusin'); h.doc.activeElement = h.doc.body;
  h.lose(); h.restore(); await h.finish();
  assert.equal(count(h, 'focus'), 0); assert.equal(h.doc.activeElement, h.doc.body);
  h.tick(); assert.equal(count(h, 'frame'), 1);
});

for (const kind of ['multiplayer', 'replay', 'content', 'material']) {
  test(`${kind} import completion after fatal recovery cannot create tools or finish boot`, async () => {
    const h = fixture({search: `?${kind === 'multiplayer' ? kind : kind + '-tools'}`});
    h.resolveLoad(); await flush();
    const loading = h.modules.get(`./${kind}-tools.js`); assert.ok(loading);
    assert.equal(count(h, 'init'), 1, 'Wasm initialized while the host was still alive');
    h.failRecovery(); const failure = h.status.textContent;
    loading.resolve(h.module(kind)); await h.boot;
    assert.equal(count(h, 'create-tool'), 0);
    assert.equal(h.status.textContent, failure); assert.equal(h.controls.disabled, true);
    assert.equal(h.multiplayer.disabled, true); assert.equal(count(h, 'focus'), 0);
    assert.equal(h.failures.length, 1); detached(h);
  });
}

test('permanent pagehide during an optional import prevents later factories and button handlers', async () => {
  const h = fixture({search: '?multiplayer&replay-tools'}); h.resolveLoad(); await flush();
  h.modules.get('./multiplayer-tools.js').resolve(h.module('multiplayer')); await flush();
  assert.ok(h.modules.has('./replay-tools.js'));
  h.close(); const previous = h.status.textContent;
  h.modules.get('./replay-tools.js').resolve(h.module('replay')); await h.boot;
  assert.equal(count(h, 'create-tool'), 1, 'only the tool created while alive exists');
  assert.deepEqual(h.calls.filter(row => row[0] === 'dispose-tool'), [['dispose-tool', 'multiplayer']]);
  h.emit(h.controls, 'click'); h.emit(h.multiplayer, 'click'); h.close();
  assert.equal(count(h, 'controls'), 0); assert.equal(count(h, 'toggle-tool'), 0);
  assert.equal(count(h, 'dispose-tool'), 1); assert.equal(h.status.textContent, previous);
  assert.equal(h.controls.disabled, true); assert.equal(h.multiplayer.disabled, true);
  assert.deepEqual(h.failures, []); detached(h);
});

for (const outcome of ['success', 'rejection']) test(`lazy multiplayer import after boot ignores late ${outcome} after teardown`, async () => {
  const h = fixture(); await h.finish(); h.emit(h.multiplayer, 'click');
  const loading = h.modules.get('./multiplayer-tools.js'); assert.ok(loading);
  h.failRecovery(); const failure = h.status.textContent;
  if (outcome === 'success') loading.resolve(h.module('multiplayer'));
  else loading.reject(Error('late multiplayer import failure'));
  await flush();
  assert.equal(count(h, 'create-tool'), 0); assert.equal(h.status.textContent, failure);
  assert.equal(h.failures.length, 1); assert.equal(h.controls.disabled, true);
  assert.equal(h.multiplayer.disabled, true); detached(h);
});

test('a late optional import rejection preserves the earlier graphics failure', async () => {
  const h = fixture({search: '?replay-tools'}); h.resolveLoad(); await flush();
  h.failRecovery(); const failure = h.status.textContent;
  h.modules.get('./replay-tools.js').reject(Error('late import failure')); await h.boot;
  assert.equal(h.status.textContent, failure); assert.deepEqual(h.loadErrors, []);
  assert.equal(h.failures.length, 1); detached(h);
});

test('an optional boot import failure on a live host remains actionable', async () => {
  const h = fixture({search: '?content-tools'}); h.resolveLoad(); await flush();
  assert.equal(h.status.textContent, 'Starting game…');
  const cause = Error('content tools failed to load');
  h.modules.get('./content-tools.js').reject(cause); await h.boot;
  assert.equal(h.status.textContent, cause.message);
  assert.deepEqual(h.loadErrors, [cause]); assert.deepEqual(h.failures, [cause]);
  assert.equal(h.controls.disabled, true); assert.equal(h.multiplayer.disabled, true);
  detached(h);
});

test('optional imports can complete during BFCache suspension and run after pageshow', async t => {
  const h = fixture({search: '?multiplayer&replay-tools&content-tools&material-tools'}); t.after(h.close);
  h.resolveLoad(); await flush(); h.emit(h.win, 'pagehide', {persisted: true});
  const suspended = h.status.textContent;
  for (const kind of ['multiplayer', 'replay', 'content', 'material']) {
    h.modules.get(`./${kind}-tools.js`).resolve(h.module(kind)); await flush();
  }
  await h.boot;
  assert.equal(count(h, 'create-tool'), 4); assert.equal(h.status.textContent, suspended);
  assert.equal(h.pending.size, 0); assert.equal(count(h, 'dispose-player'), 0);
  h.emit(h.win, 'pageshow', {persisted: true}); h.tick();
  assert.equal(count(h, 'frame'), 1); assert.equal(count(h, 'focus'), 0);
  assert.deepEqual(h.failures, []);
});

test('teardown during initialization stops remaining boot stages', async () => {
  const h = fixture(); h.onInit = h.close; await h.finish();
  assert.equal(count(h, 'init'), 1); assert.equal(count(h, 'autoplay'), 0);
  assert.equal(count(h, 'preferences'), 0); assert.equal(h.controls.disabled, true);
  detached(h);
});
