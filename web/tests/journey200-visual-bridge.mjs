// Executes current host sources with the real visual/lifecycle helpers. Browser,
// rendering and Wasm doubles establish ordering/contracts, not game consumers.
import assert from 'node:assert/strict';
import {getEventListeners} from 'node:events';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindVisualPreferences, configureVisuals} from '../visuals.js';
import {bindGraphicsLifecycle} from '../lifecycle.js';
import {framebufferSize} from '../details.js';

const hosts = {standard: '../runtime.js', courtyard: '../owned/platform.js', orbit: '../owned-orbit/platform.js'};
async function fixture(kind, {search = '', missing = false, missingQuality = false, loadFails = false} = {}) {
  const trace = [], failures = [], overrides = [], nodes = new Map(), pending = new Map();
  let sequence = 0, timestamp = 0, nextQuality, quality = 2, override, reply = 1, running = 1;
  let policy = [false, false], choices = {reducedMotion: 'system', reducedFlash: false};
  let playerDisposed = 0, audioDisposed = 0, pollCount = 0, frameThrows = false;
  const media = {matches: false};
  class Node extends EventTarget {
    constructor() {
      super(); this.dataset = {}; this.style = {}; this.attributes = new Map();
      this.width = this.clientWidth = 640; this.height = this.clientHeight = 480;
      this.value = '0'; this.textContent = ''; this.hidden = false; this.disabled = true;
    }
    getBoundingClientRect() { return {width: this.clientWidth, height: this.clientHeight, left: 0, top: 0}; }
    setAttribute(name, value) { this.attributes.set(name, String(value)); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    toggleAttribute() {}
    append() {}
    focus() {}
    getContext(type) { return type === '2d' ? {putImageData: () => trace.push(['draw'])} : gl; }
  }
  const win = new Node(), doc = new Node(), canvas = new Node();
  win.matchMedia = () => media;
  doc.defaultView = win; doc.body = new Node(); doc.hidden = false; doc.hasFocus = () => true;
  canvas.ownerDocument = doc; canvas.dataset.game = 'neon-swarm';
  const node = id => { if (!nodes.has(id)) nodes.set(id, new Node()); return nodes.get(id); };
  nodes.set('game', canvas);
  doc.getElementById = node;
  doc.querySelector = selector => selector === 'canvas' || selector === '#game' ? canvas
    : ['[data-multiplayer]'].includes(selector) ? null : node(selector);
  doc.createElement = () => new Node();
  const emit = (target, type, props = {}) => target.dispatchEvent(Object.assign(new Event(type, {cancelable: true}), props));
  const gl = new Proxy({}, {get: (_, key) => {
    if (String(key).toUpperCase() === key) return 1;
    if (key === 'getParameter') return () => 8192;
    if (key === 'getExtension') return () => null;
    if (['getShaderParameter', 'getProgramParameter'].includes(key)) return () => true;
    if (key === 'checkFramebufferStatus') return () => 1;
    return () => ({});
  }});
  const effective = () => override ?? quality;
  function frame(...args) {
    if (frameThrows) throw Error('frame failed');
    trace.push(['frame', ...policy, effective(), ...args]);
    if (nextQuality !== undefined) { quality = nextQuality; nextQuality = undefined; }
    return running;
  }
  function feedback(motion, flash) {
    assert.equal(typeof motion, 'number'); assert.equal(typeof flash, 'number');
    assert.ok(motion === 0 || motion === 1); assert.ok(flash === 0 || flash === 1);
    trace.push(['apply', motion, flash, effective(), canvas.width, canvas.height]);
    if (reply === 1) policy = [!!motion, !!flash];
    return reply;
  }
  const engine = {
    memory: {buffer: new ArrayBuffer(640 * 720 * 4)},
    ferravex_init() {}, ferravex_autoplay() {},
    ferravex_frame: frame, ferravex_controls_step: frame, orbit_controls_frame: frame,
    ferravex_visual_quality: effective, ferravex_visual_look: () => 0,
    ferravex_quality_override(value) { overrides.push(value); override = value; },
    ferravex_look_override() {}, ferravex_feedback_preferences: feedback,
    ferravex_ui_preferences_v1: () => 1,
    ferravex_details_set(field, value) { assert.equal(field, 0); nextQuality = value - 1; return 1; },
    ferravex_details_get: () => 0, ferravex_resolution: () => 1,
    ferravex_vertex_count: () => 0, ferravex_vertices: () => 0, ferravex_batch_count: () => 0, ferravex_batches: () => 0,
    ferravex_command_count: () => 0, ferravex_commands: () => 0, ferravex_clear: () => 0,
    ferravex_culled_count: () => 0, ferravex_visible_count: () => 0,
    ferravex_training_init() {}, ferravex_training_state: () => 0, ferravex_training_progress: () => 0,
    ferravex_menu_requested: () => 0, ferravex_mesh_count: () => 0,
    ferravex_object_count: () => 2, ferravex_object: () => 0, ferravex_shadow_detail: () => 0,
    ferravex_light: () => 128, ferravex_matrix: () => 64, ferravex_status: () => 0, ferravex_status_len: () => 11,
    ferravex_instances: () => 0, ferravex_instances_len: () => 0,
    ferravex_focus() {}, ferravex_blur() {}, ferravex_training_release_input() {}, ferravex_peer_mode() {},
    ferravex_peer_out: () => 256, ferravex_peer_out_len: () => 1,
    init() {}, width: () => 640, height: () => 480, pixels: () => 0,
    orbit_paused: () => 1, orbit_pause() {}, orbit_restart() {},
    status: () => 0, score: () => 0, lives: () => 3, sector: () => 1,
  };
  if (missing) delete engine.ferravex_feedback_preferences;
  if (missingQuality) delete engine.ferravex_visual_quality;
  const player = {
    get presentationPreferences() { return Object.freeze({...choices}); },
    get uiPreferencesV1() { return {version: 1, textScale: 1, contrast: 0}; },
    shouldRender: () => true, frame() {}, dispose() { playerDisposed++; },
  };
  let visuals, peer;
  const audio = {sync() {}, suspend() {}, resume() {}, mute() {}, dispose() { audioDisposed++; }};
  const sandbox = {
    window: win, document: doc, location: {search}, devicePixelRatio: 1, URLSearchParams, performance,
    console: {error: error => failures.push(error)}, ImageData: class {},
    bindVisualPreferences: (...args) => (visuals = bindVisualPreferences(...args)), configureVisuals, bindGraphicsLifecycle,
    framebufferSize, bindDetails: () => ({player, ready() {}, update() {}}), bindAudio: () => audio,
    bindInput: () => ({imports: {}, poll() { pollCount++; }, syncPanel: () => true, suspend() {}, resume() {}}),
    loadWasm: async () => { if (loadFails) throw Error('load failed'); return {instance: {exports: engine}}; },
    showLoadError: error => failures.push(error), fetch: async () => ({ok: true, text: async () => 'shader'}),
    requestAnimationFrame: fn => { const id = sequence++; pending.set(id, fn); return id; },
    cancelAnimationFrame: id => pending.delete(id),
    createInputOverlay: () => () => {}, drawInput2d() {},
    RetainedRenderer: class {
      constructor() { this.environment = {quality: effective()}; this.meshes = new Map(); }
      begin() { this.environment.quality = effective(); } renderShadow() {}
    },
    SpriteRenderer: class { begin() {} }, GpuTimer: class { begin() {} end() {} },
    PeerLink: class {
      constructor(callbacks) { peer = this; this.callbacks = callbacks; this.mode = 0; this.sent = 0; }
      send() { this.sent++; } close() { this.mode = 0; this.callbacks.disconnected(); }
    },
  };
  // The production lifecycle takes request/cancel from its own global realm.
  sandbox.bindGraphicsLifecycle = (target, options) => bindGraphicsLifecycle(target, {
    ...options, request: sandbox.requestAnimationFrame, cancel: sandbox.cancelAnimationFrame,
  });
  const source = readFileSync(new URL(hosts[kind], import.meta.url), 'utf8').replace(/^import .*;$/gm, '');
  await runInNewContext(`(async()=>{${source}\n})()`, sandbox);
  function tick(dt = 16) {
    assert.equal(pending.size, 1, 'one host frame is scheduled');
    const [id, callback] = pending.entries().next().value; pending.delete(id);
    callback(timestamp += dt);
  }
  return {
    canvas, win, doc, media, trace, overrides, engine, visuals, node, emit, tick, failures, pending, feedback,
    get playerDisposed() { return playerDisposed; }, get audioDisposed() { return audioDisposed; },
    get quality() { return quality; }, get pollCount() { return pollCount; },
    set choices(value) { choices = value; }, set reply(value) { reply = value; },
    set running(value) { running = value; }, set frameThrows(value) { frameThrows = value; },
    get peer() { return peer; }, teardown() { emit(win, 'pagehide', {persisted: false}); },
  };
}

for (const kind of Object.keys(hosts)) {
  test(`${kind}: live system/explicit motion and independent flash reach the ABI before paused/menu frames`, async () => {
    const h = await fixture(kind);
    h.tick();
    const steps = h.trace.filter(row => row[0] === 'frame').length;
    h.media.matches = true;
    h.trace.length = 0; h.tick(0);
    assert.deepEqual(h.trace[0].slice(0, 4), ['apply', 1, 0, 2]);
    assert.deepEqual(h.trace.find(row => row[0] === 'frame').slice(1, 3), [true, false]);
    assert.equal(steps, 1);
    h.choices = {reducedMotion: false, reducedFlash: true};
    h.trace.length = 0; h.tick(0);
    assert.deepEqual(h.trace[0].slice(0, 4), ['apply', 0, 1, 2]);
    assert.deepEqual(h.trace.find(row => row[0] === 'frame').slice(1, 3), [false, true]);
    h.choices = {reducedMotion: true, reducedFlash: false}; h.media.matches = false;
    h.trace.length = 0; h.tick(0);
    assert.deepEqual(h.trace[0].slice(0, 4), ['apply', 1, 0, 2]);
    assert.equal(h.pollCount, 4, 'local menus never gate host frame/input sampling');
    assert.deepEqual(h.failures, []);
    h.teardown();
  });

  test(`${kind}: queued presets invalidate effective quality without overwriting URL priority or stored choices`, async () => {
    const h = await fixture(kind);
    h.tick(); h.trace.length = 0;
    h.engine.ferravex_details_set(0, 1); h.tick();
    assert.deepEqual(h.trace.filter(row => row[0] === 'apply').map(row => row[3]), [0]);
    assert.ok(h.trace.findIndex(row => row[0] === 'frame') < h.trace.findIndex(row => row[0] === 'apply'), 'post-frame refresh sees queued preset');
    assert.equal(h.visuals.current.quality, 0);
    h.teardown();
    const masked = await fixture(kind, {search: '?quality=low'});
    masked.tick(); masked.trace.length = 0;
    masked.engine.ferravex_details_set(0, 2); masked.tick();
    assert.equal(masked.quality, 1, 'requested choice is retained underneath URL override');
    assert.equal(masked.visuals.current.quality, 0);
    assert.deepEqual(masked.overrides, [0], 'preference refresh never reconfigures URL overrides');
    assert.equal(masked.trace.filter(row => row[0] === 'apply').length, 0);
    assert.equal(masked.canvas.dataset.visualPreferencesError, '');
    masked.teardown();
  });

  test(`${kind}: missing, rejected and nonnumeric success ABI results remain unapplied and retry without stopping frames`, async () => {
    const h = await fixture(kind, {missing: true});
    h.tick();
    assert.equal(h.visuals.current, null);
    assert.match(h.canvas.dataset.visualPreferencesError, /Missing visual export: ferravex_feedback_preferences/);
    h.engine.ferravex_feedback_preferences = h.feedback;
    for (const result of [0, true, undefined, 2, Promise.resolve(1)]) {
      h.reply = result; h.tick();
      assert.equal(h.visuals.current, null);
      assert.match(h.canvas.dataset.visualPreferencesError, /rejected/);
    }
    h.reply = 1; h.tick();
    assert.deepEqual(h.visuals.current, {reducedMotion: false, reducedFlash: false, quality: 2});
    assert.equal(h.canvas.dataset.visualPreferencesError, '');
    const previous = h.visuals.current;
    h.choices = {reducedMotion: true, reducedFlash: true}; h.reply = 0; h.tick();
    assert.equal(h.visuals.current, previous, 'failed application preserves last success');
    h.reply = 1; h.tick();
    assert.equal(h.visuals.current.reducedFlash, true);
    h.engine.ferravex_feedback_preferences = () => { throw Error('consumer unavailable'); };
    h.choices = {reducedMotion: false, reducedFlash: false}; h.tick();
    assert.equal(h.visuals.current.reducedFlash, true);
    assert.match(h.canvas.dataset.visualPreferencesError, /consumer unavailable/);
    h.engine.ferravex_feedback_preferences = h.feedback; h.tick();
    assert.equal(h.visuals.current.reducedFlash, false);
    assert.deepEqual(h.failures, []);
    h.teardown();
    const legacy = await fixture(kind, {missingQuality: true});
    legacy.tick();
    assert.equal(legacy.visuals.current, null);
    assert.match(legacy.canvas.dataset.visualPreferencesError, /Missing visual export: ferravex_visual_quality/);
    assert.deepEqual(legacy.failures, []);
    legacy.teardown();
  });

  test(`${kind}: resize and same-size renderer replacement reapply; BFCache keeps listeners until real teardown`, async () => {
    const h = await fixture(kind);
    h.tick(); h.trace.length = 0;
    h.canvas.clientWidth = 800; h.emit(h.win, 'resize'); h.tick();
    assert.ok(h.trace.some(row => row[0] === 'apply'));
    h.trace.length = 0;
    if (kind === 'orbit') {
      h.engine.memory.buffer = new ArrayBuffer(h.engine.memory.buffer.byteLength);
      h.tick();
      assert.ok(h.trace.findIndex(row => row[0] === 'apply') < h.trace.findIndex(row => row[0] === 'draw'));
    } else {
      h.emit(h.canvas, 'webglcontextlost');
      h.choices = {reducedMotion: true, reducedFlash: true};
      assert.equal(h.visuals.refresh(), true);
      assert.equal(h.trace.length, 0, 'do not apply into a lost renderer');
      h.emit(h.canvas, 'webglcontextrestored');
      assert.ok(h.trace.some(row => row[0] === 'apply'), 'reconstruction applies synchronously even at the same size');
      h.tick();
      if (kind === 'courtyard') {
        h.trace.length = 0; h.node('reload').onclick();
        assert.ok(h.trace.some(row => row[0] === 'apply'), 'same-size shader replacement forces refresh');
      }
    }
    const listeners = getEventListeners(h.win, 'resize').length;
    assert.equal(listeners, 1);
    h.emit(h.win, 'pagehide', {persisted: true});
    assert.equal(h.playerDisposed, 0);
    assert.equal(getEventListeners(h.win, 'resize').length, listeners);
    h.media.matches = true;
    h.choices = {reducedMotion: 'system', reducedFlash: false};
    h.emit(h.win, 'pageshow', {persisted: true}); h.tick();
    assert.equal(h.visuals.current.reducedMotion, true);
    h.teardown();
    assert.equal(getEventListeners(h.win, 'resize').length, 0);
    assert.equal(getEventListeners(h.win, 'pagehide').length, 0);
    for (const type of ['webglcontextlost', 'webglcontextrestored']) assert.equal(getEventListeners(h.canvas, type).length, 0);
    assert.equal(h.playerDisposed, 1);
    assert.equal(h.audioDisposed, 1);
    assert.equal(h.visuals.refresh(true), false);
    const count = h.trace.length;
    h.emit(h.win, 'pageshow', {persisted: true});
    if (h.pending.size) h.tick();
    assert.equal(h.trace.length, count, 'queued callbacks stay inert after teardown');
    h.teardown(); assert.equal(h.playerDisposed, 1);
    assert.deepEqual(h.failures, []);
  });

  test(`${kind}: boot and fatal frame failures dispose bridge listeners`, async () => {
    const failed = await fixture(kind, {loadFails: true});
    assert.match(failed.failures[0].message, /load failed/);
    assert.equal(getEventListeners(failed.win, 'resize').length, 0);
    assert.equal(failed.visuals.refresh(), false);
    const h = await fixture(kind); h.frameThrows = true; h.tick();
    assert.match(h.failures[0].message, /frame failed/);
    assert.equal(getEventListeners(h.win, 'resize').length, 0);
    assert.equal(h.visuals.refresh(), false);
    assert.equal(h.playerDisposed, 1);
  });
}

test('standard normal completion disposes preferences and stops graphics scheduling', async () => {
  const h = await fixture('standard'); h.running = 0; h.tick();
  assert.equal(h.pending.size, 0);
  assert.equal(h.visuals.refresh(), false);
  assert.equal(h.playerDisposed, 1);
  assert.equal(getEventListeners(h.canvas, 'webglcontextrestored').length, 0);
});

test('Courtyard keeps peer progression live under the local menu when feedback ABI is absent', async () => {
  const h = await fixture('courtyard', {missing: true});
  h.peer.mode = 1; h.peer.callbacks.connected(1);
  h.tick(64); h.tick(64);
  assert.equal(h.trace.filter(row => row[0] === 'frame').length, 2);
  assert.ok(h.peer.sent > 0);
  assert.equal(h.visuals.current, null);
  assert.deepEqual(h.failures, []);
  h.teardown();
});
