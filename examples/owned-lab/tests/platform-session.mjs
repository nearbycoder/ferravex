import {bindVisualPreferences} from '../../../web/visuals.js';
// Exercise the actual custom host with first-party browser/engine mocks. Rust tests
// verify the simulation; this checks browser lifecycle and capture failure routing.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const html = fs.readFileSync(new URL('../../../web/owned/index.html', import.meta.url), 'utf8');
const source = fs.readFileSync(new URL('../../../web/owned/platform.js', import.meta.url), 'utf8')
  .replace(/^import .*;$/gm, '');
class Element {
  constructor() {
    this.handlers = new Map(); this.hidden = false; this.disabled = false;
    this.textContent = ''; this.value = ''; this.dataset = {};
    this.clientWidth = this.width = 960; this.clientHeight = this.height = 540;
  }
  addEventListener(type, handler) {
    if (!this.handlers.has(type)) this.handlers.set(type, []);
    this.handlers.get(type).push(handler);
  }
  removeEventListener(type, handler) { this.handlers.set(type, (this.handlers.get(type) ?? []).filter(fn => fn !== handler)); }
  emit(type, event = {}) { for (const fn of this.handlers.get(type) ?? []) fn({preventDefault() {}, ...event}); }
  focus() {} setAttribute() {} toggleAttribute() {} append() {}
  getBoundingClientRect() { return {width: 960, height: 540}; }
}
const nodes = new Map([...html.matchAll(/id="([^"]+)"/g)].map(([, id]) => [id, new Element()]));
const $ = id => { assert.ok(nodes.has(id), `Host references missing element ${id}`); return nodes.get(id); };
const window = new Element(), document = new Element();
document.getElementById = $; document.createElement = () => new Element(); document.body = new Element();
document.hasFocus = () => true;
const canvas = $('game');
const gl = new Proxy({}, {get: (_, key) => {
  if (String(key).toUpperCase() === key) return 1;
  if (key === 'getParameter') return () => 8192;
  if (key === 'getShaderParameter' || key === 'getProgramParameter') return () => true;
  if (key === 'checkFramebufferStatus') return () => 1;
  return () => ({});
}});
canvas.getContext = () => gl;
let captureFails = false;
const events = [], frames = [];
canvas.requestPointerLock = () => {
  events.push('capture');
  if (captureFails) return Promise.reject(Error('denied'));
  document.pointerLockElement = canvas; document.emit('pointerlockchange');
  return Promise.resolve();
};
document.exitPointerLock = () => { document.pointerLockElement = null; document.emit('pointerlockchange'); };
let state = 0, progress = 0, mode = 0, panel = false, requestedMenu = 0, resetCount = 0;
const engine = {
  memory: {buffer: new ArrayBuffer(4096)},
  ferravex_init() { throw Error('Interactive host must use Ready initialization'); },
  ferravex_training_init() { state = 0; },
  ferravex_training_state: () => state,
  ferravex_training_progress: () => progress,
  ferravex_training_reset() { resetCount++; state = progress = 0; return 1; },
  ferravex_training_release_input() { events.push('release'); },
  ferravex_controls_step(...args) {
    frames.push(args);
    if (args[1] || mode) state = 1;
    else if (state === 1) state = 2;
  },
  ferravex_focus() {}, ferravex_blur() {},
  ferravex_controls_open: () => panel, ferravex_menu_requested: () => requestedMenu,
  ferravex_mesh_count: () => 0, ferravex_object_count: () => 2, ferravex_object: () => 0,
  ferravex_resolution: () => 1, ferravex_shadow_detail: () => 0,
  ferravex_light: () => 128, ferravex_matrix: () => 64,
  ferravex_instances: () => 0, ferravex_instances_len: () => 0,
  ferravex_status: () => 0, ferravex_status_len: () => 11,
  ferravex_visual_look: () => 0, ferravex_visual_quality: () => 0,
  ferravex_peer_mode(value) { mode = value; }, ferravex_peer_out: () => 256, ferravex_peer_out_len: () => 0,
};
let peerCallbacks, peer, nextFrame, nextPair;
const errors = [];
await vm.runInNewContext(`(async()=>{${source}\n})()`, {
  window, document, console, performance, location: {search: ''}, devicePixelRatio: 1,
  matchMedia: () => ({matches: true}), requestAnimationFrame: fn => { nextFrame = fn; },
  fetch: async () => ({ok: true, text: async () => 'shader'}),
  loadWasm: async () => ({instance: {exports: engine}}),
  showLoadError: error => errors.push(error), configureVisuals() {}, bindVisualPreferences,
  bindInput: () => ({imports: {}, poll() {}, syncPanel: () => panel, openSettings() { panel = true; },
    suspend() { events.push('input-suspend'); }, resume() { events.push('input-resume'); }}),
  bindAudio: () => ({sync() {}, mute() {}, enable() { events.push('audio'); return Promise.resolve(); }}),
  bindDetails: () => ({ready() {}, update() {}}),
  framebufferSize: () => ({width: 960, height: 540, scale: 1}),
  createInputOverlay: () => () => {}, GpuTimer: class { begin() {} end() {} },
  PeerLink: class {
    constructor(callbacks) { peer = this; peerCallbacks = callbacks; this.mode = 0; }
    close() { this.mode = 0; peerCallbacks.disconnected(); }
    host() { return nextPair; }
    join() { return nextPair; }
    accept() { return nextPair; }
    send() {}
  },
});
let now = 0;
function frame() { assert.ok(nextFrame); nextFrame(now += 16); assert.deepEqual(errors, []); }
assert.equal($('play').disabled, false);
assert.match($('menu-status').textContent, /Ready/);
frame(); assert.equal(frames.at(-1)[1], 0, 'initial menu supplies no gameplay input');
captureFails = true; $('play').onclick(); await Promise.resolve(); frame();
assert.equal(state, 0);
assert.match($('menu-status').textContent, /capture failed/, 'first capture failure is visible before training starts');
captureFails = false; events.length = 0;
$('play').onclick(); await Promise.resolve();
assert.deepEqual(events.slice(0, 2), ['capture', 'audio'], 'capture precedes asynchronous audio');
frame(); assert.equal(frames.at(-1)[1], 1);
assert.match($('capture-status').textContent, /Mouse captured/);
window.emit('blur'); frame();
assert.equal(frames.at(-1)[1], 0); assert.equal(state, 2);
assert.ok(events.includes('release'), 'pause discards pending actions immediately');
assert.match($('menu-status').textContent, /Focus left/);
window.emit('focus'); frame(); assert.equal(frames.at(-1)[1], 0, 'focus return needs explicit resume');

$('play').onclick(); frame(); document.exitPointerLock(); frame();
assert.equal(frames.at(-1)[1], 0, 'pointer release pauses training');
captureFails = true; $('play').onclick(); await Promise.resolve(); frame();
assert.match($('menu-status').textContent, /capture failed/);
assert.equal(frames.at(-1)[1], 0);
captureFails = false;
$('overview').onclick(); frame(); assert.equal(frames.at(-1)[2], 2);
document.hidden = true; document.emit('visibilitychange'); frame();
assert.equal(frames.at(-1)[1], 0);
document.hidden = false; document.emit('visibilitychange'); frame();
assert.equal(frames.at(-1)[1], 0, 'tab return needs explicit resume');
$('overview').onclick(); frame(); $('controls').onclick(); frame();
panel = false; frame(); assert.equal(frames.at(-1)[1], 0, 'closing settings leaves training paused');

$('overview').onclick(); frame();
const beforeGraphicsLoss = frames.length;
canvas.emit('webglcontextlost');
assert.equal(events.at(-1), 'input-suspend', 'graphics loss clears bridge contact ownership');
frame(); assert.equal(frames.length, beforeGraphicsLoss, 'lost graphics skips rendering and controls stepping');
canvas.emit('webglcontextrestored');
assert.equal(events.at(-1), 'input-resume', 'successful graphics setup restores bridge availability');
frame(); assert.equal(frames.at(-1)[1], 0, 'graphics recovery retains the explicit gameplay resume requirement');

progress = 15; frame(); assert.match($('menu-objective').textContent, /4 \/ 4 drills complete/);
$('edit').onclick(); $('reset').onclick(); frame();
assert.equal(resetCount, 1); assert.equal(state, 0);
assert.match($('menu-objective').textContent, /0 \/ 4/);
assert.match($('play').textContent, /Enter training/);
peer.mode = 1; peerCallbacks.connected(1); frame();
assert.equal(frames.at(-1)[1], 0, 'network menu still releases local input');
assert.equal(mode, 1, 'host keeps the authoritative peer mode');
assert.match($('menu-status').textContent, /Match stays live/);
assert.equal($('reset').disabled, true);
$('reset').onclick(); assert.equal(resetCount, 1, 'live peer reset cannot reach engine');
peer.close(); frame(); assert.equal(state, 2); assert.equal($('reset').disabled, false);

function beginPair(button) {
  let resolve, reject;
  nextPair = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {resolve, reject, done: $(button).onclick()};
}
const oldPair = beginPair('hostPeer'), newPair = beginPair('joinPeer');
newPair.resolve('current token'); await newPair.done;
const currentMessage = $('peerStatus').textContent;
oldPair.reject(Error('obsolete failure')); await oldPair.done;
assert.equal($('peerToken').value, 'current token');
assert.equal($('peerStatus').textContent, currentMessage, 'older rejection cannot overwrite a newer attempt');

for (const reject of [false, true]) {
  const pending = beginPair('hostPeer');
  $('leavePeer').onclick();
  if (reject) pending.reject(Error('late failure'));
  else pending.resolve('obsolete token');
  await pending.done;
  assert.equal($('peerToken').value, 'current token', 'leave discards stale tokens');
  assert.equal($('peerStatus').textContent, 'Offline practice', 'leave discards late outcomes');
}
const resetPair = beginPair('joinPeer');
$('reset').onclick();
peerCallbacks.status('Pairing closed');
resetPair.resolve('pre-reset token'); await resetPair.done;
assert.equal($('peerToken').value, 'current token');
assert.equal($('peerStatus').textContent, 'Pairing closed', 'training reset invalidates pending UI writes');

for (const reject of [false, true]) {
  const connecting = beginPair('acceptPeer');
  peer.mode = 1; peerCallbacks.connected(1); peerCallbacks.status('Connected over browser DTLS.');
  if (reject) connecting.reject(Error('late negotiation failure'));
  else connecting.resolve();
  await connecting.done;
  assert.equal($('peerStatus').textContent, 'Connected over browser DTLS.', 'channel-open status wins over pending pairing');
  peer.close();
}
const cancelled = beginPair('hostPeer');
peerCallbacks.status('Pairing cancelled');
cancelled.reject(Object.assign(Error('obsolete abort'), {name: 'AbortError'})); await cancelled.done;
assert.equal($('peerStatus').textContent, 'Pairing cancelled', 'intentional cancellation keeps the helper status');
const failedPair = beginPair('joinPeer');
failedPair.reject(Error('Invalid answer token')); await failedPair.done;
assert.equal($('peerStatus').textContent, 'Invalid answer token', 'current actionable failures remain visible');
assert.deepEqual(errors, []);
console.log('Courtyard browser session routing passed.');
