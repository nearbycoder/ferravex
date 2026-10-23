import {bindVisualPreferences} from '../visuals.js';
// Source and actual host-handler checks; no dist artifacts, Wasm execution,
// browser layout or assistive-technology claims. Run this file directly with Node.
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const hostUrl = new URL('../owned/index.html', import.meta.url);
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const html = readFileSync(hostUrl, 'utf8');
const platform = read('../owned/platform.js');
const interactive = read('../../examples/owned-lab/src/interactive.rs');
const presets = read('../../crates/ferravex-render/src/input/presets.rs');
const markup = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, '');
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const script = html.match(/<script id="courtyard-accessibility">([\s\S]*?)<\/script>/)[1];
const plain = value => value.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
const attrs = tag => Object.fromEntries([...tag.matchAll(/([\w:-]+)(?:="([^"]*)")?/g)].slice(1).map(([, name, value]) => [name, value ?? true]));
const tags = name => [...markup.matchAll(new RegExp(`<${name}\\b[^>]*>`, 'g'))].map(match => attrs(match[0]));
function text(id) {
  const match = markup.match(new RegExp(`<([\\w-]+)\\b[^>]*\\bid="${id}"[^>]*>([\\s\\S]*?)<\\/\\1>`));
  assert.ok(match, `#${id} exists`);
  return plain(match[2]);
}
const descriptionIds = ['courtyard-intro', 'courtyard-keyboard', 'courtyard-remapping', 'courtyard-session'];
const baselineIds = 'game audio controls top hud vitals score crosshair training objective capture-status pause menu menu-status menu-objective play overview edit tools closeTools reset object x y z apply save load record download fog shader reload hostPeer joinPeer acceptPeer leavePeer peerToken peerStatus peerStats metrics message'.split(' ');

test('canvas name, descriptions, landmarks and fallback identify the actual game', () => {
  assert.equal(tags('html')[0].lang, 'en');
  assert.equal(plain(markup.match(/<title>(.*?)<\/title>/)[1]), 'Ferravex — Courtyard');
  assert.equal(text('courtyard-title'), 'Courtyard');
  assert.equal(tags('main').length, 1);
  assert.equal(tags('canvas').length, 1);
  const [canvas] = tags('canvas');
  assert.equal(canvas.id, 'game');
  assert.equal(canvas.tabindex, '0');
  assert.equal(canvas['aria-label'], 'Courtyard game canvas');
  assert.equal(canvas.role, undefined);
  assert.equal(canvas['aria-hidden'], undefined);
  assert.deepEqual(canvas['aria-describedby'].split(' '), descriptionIds);
  for (const id of descriptionIds) assert.ok(text(id).length > 0, `${id} has linked instructions`);
  assert.equal(text('courtyard-intro'), 'Move, aim and complete four training drills.');
  assert.match(markup, /<details class="courtyard-guide"><summary>How to play<\/summary>/, 'long instructions are available on demand');
  assert.match(text('game'), /JavaScript, WebAssembly and WebGL2/);
  assert.equal(tags('noscript').length, 1);
  assert.match(tags('meta').find(meta => meta.name === 'viewport').content, /width=device-width/);
  assert.doesNotMatch(html, /user-scalable=no|maximum-scale=1/);
  const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const id of baselineIds) assert.ok(ids.includes(id), `preserve custom hook #${id}`);
  const labelled = tags('section').concat(tags('aside'));
  assert.equal(labelled.find(tag => tag.id === 'menu')['aria-labelledby'], 'courtyard-title');
  assert.equal(labelled.find(tag => tag.id === 'tools')['aria-labelledby'], 'courtyard-tools-title');
  const labels = tags('label');
  for (const field of [...tags('input'), ...tags('select'), ...tags('textarea')]) {
    assert.ok(labels.some(label => label.for === field.id), `${field.id} keeps a real label`);
  }
});

test('guide follows Shooter defaults, explicit resume, remapping and live peer semantics', () => {
  assert.match(interactive, /PlayerInput::preset\("owned-lab", Preset::Shooter\)/);
  for (const [name, a, b] of [['move_left', 'A', 'Left'], ['move_right', 'D', 'Right'], ['move_forward', 'W', 'Up'], ['move_back', 'S', 'Down']]) {
    assert.match(presets, new RegExp(`"${name}",[^\\n]+KeyCode::${a}, KeyCode::${b}`));
  }
  assert.match(presets, /key\(KeyCode::Space\)[\s\S]*?add\("jump"/);
  assert.match(presets, /"reload",[\s\S]*?key\(KeyCode::R\)/);
  assert.match(presets, /bindings\.push\(b\(InputCode::Mouse\(1\)\)\)[\s\S]*?add\("fire"/);
  assert.match(presets, /"confirm",[\s\S]*?key\(KeyCode::Enter\)/);
  assert.match(presets, /let pause_key =[\s\S]*?KeyCode::Escape/);
  const guide = plain(markup);
  for (const hint of [/Default keyboard and mouse controls/, /W \/ A \/ S \/ D or arrow keys/, /Mouse to aim; left click to fire/, /Jump Space/, /Reload R/, /Scene tools → Reset/, /Escape cancels an active remap/, /Details → Player tools/]) assert.match(guide, hint);
  assert.match(text('courtyard-remapping'), /Saved or remapped bindings may differ/);
  assert.match(text('courtyard-keyboard'), /2D overview starts without mouse capture; movement stays active/);
  assert.match(text('courtyard-session'), /Offline training keeps its progress and waits for an explicit resume/);
  assert.match(text('courtyard-session'), /Connected matches stay live/);
  assert.match(text('courtyard-session'), /Closing Controls does not resume training/);
  assert.doesNotMatch(guide, /touch|swipe|tap to|keyboard.only/i, 'no unverified device/control claims');
});

test('custom boot, Wasm ABI, tools and catalog routes are preserved', () => {
  const game = JSON.parse(read('../../examples/catalog.json')).find(entry => entry.id === 'owned-lab');
  assert.equal(game.host, 'owned');
  assert.equal(game.title, 'Courtyard');
  const scripts = [...html.matchAll(/<script\b[^>]*>/g)].map(match => attrs(match[0]));
  assert.deepEqual(scripts.filter(script => script.src), [{type: 'module', src: 'platform.js'}]);
  assert.equal(scripts.length, 2);
  assert.match(platform, /loadWasm\('owned_lab\.wasm',\{env:input\.imports\}\)/);
  assert.match(platform, /wasm\.ferravex_training_init\(\)/);
  assert.doesNotMatch(platform, /wasm\.ferravex_init\(\)/);
  for (const name of ['scene.vert', 'scene.frag', 'platform.js', 'peer.js']) assert.ok(existsSync(new URL(name, hostUrl)));
  for (const module of ['loader.js', 'details.js', 'visuals.js', 'input.js', 'audio.js', 'input-overlay.js', 'retained.js']) {
    assert.ok(platform.includes(`from '../${module}'`));
    assert.ok(existsSync(new URL(`../${module}`, hostUrl)));
  }
  for (const hook of ['createInputOverlay(gl)', 'inputOverlay(wasm)', 'configureVisuals(wasm,location.search)', 'bindDetails(canvas,\'owned-lab\'', 'bindAudio(canvas', 'input.suspend()', 'input.resume()']) assert.ok(platform.includes(hook), hook);
  for (const prefix of ['/web/', '/play/']) {
    const route = new URL(`${prefix}${game.host}/?look=warm`, 'https://example.test');
    for (const file of ['platform.js', 'scene.vert', 'scene.frag', 'peer.js', 'owned_lab.wasm']) assert.equal(new URL(file, route).pathname, `${prefix}owned/${file}`);
    assert.equal(new URL('../input.js', route).pathname, `${prefix}input.js`);
    for (const link of tags('a')) assert.equal(new URL(link.href, route).pathname, prefix);
  }
  for (const button of tags('button')) assert.equal(button.type, 'button');
  for (const id of ['audio', 'controls', 'play', 'overview', 'edit']) assert.equal(tags('button').find(button => button.id === id).disabled, true, `${id} readiness stays with platform`);
  assert.deepEqual(tags('button').map(button => button.id).sort(), baselineIds.filter(id => ['audio', 'controls', 'pause', 'play', 'overview', 'edit', 'closeTools', 'reset', 'apply', 'save', 'load', 'record', 'download', 'reload', 'hostPeer', 'joinPeer', 'acceptPeer', 'leavePeer'].includes(id)).sort(), 'do not duplicate shared player buttons');
  assert.doesNotMatch(script, /ferravex_|requestAnimationFrame|setInterval|fetch\(|WebAssembly/, 'local accessibility code owns no engine or transport work');
});

test('styles allow wrapping, short-screen scroll and focus without shared absolute toolbar selectors', () => {
  assert.match(css, /\.courtyard-host :focus-visible\s*\{[^}]*outline: 3px solid/);
  assert.match(css, /#game:focus-visible\s*\{[^}]*outline-offset: -4px/);
  assert.match(css, /#courtyard-navigation\s*\{[^}]*flex-wrap: wrap[^}]*height: auto/);
  assert.match(css, /#courtyard-navigation button\s*\{[^}]*position: static[^}]*min-height: 44px/);
  assert.match(css, /#menu\s*\{[^}]*overflow: auto/);
  assert.match(css, /#tools\s*\{[^}]*overflow: auto/);
  assert.match(css, /grid-template-rows: auto minmax\(0, 1fr\)/);
  assert.match(css, /minmax\(min\(100%, 14rem\), 1fr\)/);
  assert.match(css, /#menu, #tools\s*\{[^}]*overflow-wrap: anywhere/);
  assert.match(css, /#tools \.coordinates > div\s*\{[^}]*min-width: 0/);
  assert.doesNotMatch(css, /outline: none|font-size: (?:9|11|12)px/);
  assert.match(markup, /<header>\s*<nav/, 'global body > nav rules cannot hide the catalog link');
  assert.match(markup, /<main[^>]*>\s*<canvas/, 'body > canvas fullscreen sizing cannot cover the toolbar');
});

// A bounded DOM/event fixture executes both the actual inline accessibility
// script and platform.js, with the real shared keyboard bridge. Only rendering,
// loader, audio, Details and the simulation exports are mocked.
async function fixture() {
  const observations = [], mutations = [];
  class Element {
    constructor(tag = 'div', attributes = {}) {
      this.tag = tag; this.attributes = {...attributes}; this.children = []; this.handlers = [];
      this.dataset = {}; this.value = attributes.value ?? ''; this.textContent = '';
      this.clientWidth = this.width = 960; this.clientHeight = this.height = 540;
    }
    addEventListener(type, listener, options) { this.handlers.push({type, listener, capture: options === true || !!options?.capture}); }
    removeEventListener(type, listener) { this.handlers = this.handlers.filter(row => row.type !== type || row.listener !== listener); }
    emit(type, event = {}, capture = false) {
      for (const handler of this.handlers) {
        if (handler.type === type && handler.capture === capture) handler.listener(event);
        if (event.immediate) break;
      }
    }
    getAttribute(name) { return this.attributes[name] ?? null; }
    hasAttribute(name) { return name in this.attributes; }
    setAttribute(name, value) { const oldValue = this.getAttribute(name); this.attributes[name] = String(value); mutations.push({target: this, attributeName: name, oldValue}); }
    toggleAttribute(name, force) {
      const present = this.hasAttribute(name), next = force ?? !present;
      if (next && !present) this.setAttribute(name, '');
      else if (!next && present) { const oldValue = this.getAttribute(name); delete this.attributes[name]; mutations.push({target: this, attributeName: name, oldValue}); }
    }
    get hidden() { return this.hasAttribute('hidden'); } set hidden(value) { this.toggleAttribute('hidden', value); }
    get disabled() { return this.hasAttribute('disabled'); } set disabled(value) { this.toggleAttribute('disabled', value); }
    contains(node) { return this === node || this.children.some(child => child.contains(node)); }
    append(node) { this.children.push(node); node.parent = this; }
    closest() { return ['input', 'textarea', 'select', 'button'].includes(this.tag) ? this : null; }
    focus() { if (this.disabled) return; doc.activeElement = this; doc.emit('focusin', {target: this}); }
    click() { if (!this.disabled) { this.onclick?.(); this.emit('click', {target: this}); } }
    getBoundingClientRect() { return {left: 0, top: 0, width: 960, height: 540}; }
  }
  const nodes = new Map(), stack = [];
  const voidTags = new Set(['meta', 'input', 'link', 'br', 'hr']);
  for (const match of markup.matchAll(/<(\/?)([\w-]+)\b[^>]*>/g)) {
    if (match[1]) { assert.equal(stack.pop()?.tag, match[2], 'well-nested host markup'); continue; }
    const node = new Element(match[2], attrs(match[0]));
    stack.at(-1)?.append(node);
    if (node.attributes.id) nodes.set(node.attributes.id, node);
    if (node.tag === 'body') nodes.set('body', node);
    if (!voidTags.has(node.tag)) stack.push(node);
  }
  const $ = id => { assert.ok(nodes.has(id), `platform hook #${id}`); return nodes.get(id); };
  const doc = new Element('document'), win = new Element('window'), canvas = $('game');
  doc.body = $('body'); doc.activeElement = doc.body; doc.hidden = false;
  let focused = true, dialog = null;
  doc.hasFocus = () => focused;
  doc.getElementById = $;
  doc.querySelector = selector => { assert.equal(selector, 'dialog[open]'); return dialog; };
  doc.createElement = tag => new Element(tag);
  class MutationObserver {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { observations.push({observer: this, target, options}); }
  }
  function flush() {
    let rounds = 0;
    while (mutations.length) {
      assert.ok(++rounds <= 6, 'observers must settle without a feedback loop');
      const batch = mutations.splice(0), callbacks = new Map();
      for (const {observer, target, options} of observations) {
        const matching = batch.filter(row => row.target === target && options.attributeFilter.includes(row.attributeName));
        if (matching.length) callbacks.set(observer, [...(callbacks.get(observer) ?? []), ...matching]);
      }
      for (const [observer, records] of callbacks) observer.callback(records);
    }
  }
  const calls = [], frames = [], errors = [];
  let panel = false, mode = 0, requestedMenu = 0, nextFrame, now = 0, captureFails = false;
  const engine = {
    memory: {buffer: new ArrayBuffer(4096)},
    ferravex_training_init() {}, ferravex_training_state: () => 0, ferravex_training_progress: () => 0,
    ferravex_training_reset: () => 1, ferravex_training_release_input: () => calls.push(['release']),
    ferravex_controls_step: (...args) => frames.push(args),
    ferravex_focus: value => calls.push(['focus', value]), ferravex_blur: () => calls.push(['blur']),
    ferravex_key: (...args) => calls.push(['key', ...args]),
    ferravex_input_settings: () => { panel = true; }, ferravex_controls_open: () => panel,
    ferravex_menu_requested: () => { const value = requestedMenu; requestedMenu = 0; return value; },
    ferravex_mesh_count: () => 0, ferravex_object_count: () => 2, ferravex_object: () => 0,
    ferravex_resolution: () => 1, ferravex_shadow_detail: () => 0,
    ferravex_light: () => 128, ferravex_matrix: () => 64,
    ferravex_instances: () => 0, ferravex_instances_len: () => 0,
    ferravex_status: () => 0, ferravex_status_len: () => 11,
    ferravex_visual_look: () => 0, ferravex_visual_quality: () => 0,
    ferravex_peer_mode: value => { mode = value; }, ferravex_peer_out: () => 256, ferravex_peer_out_len: () => 1,
  };
  const gl = new Proxy({}, {get: (_, key) => {
    if (String(key).toUpperCase() === key) return 1;
    if (key === 'getParameter') return () => 8192;
    if (['getShaderParameter', 'getProgramParameter'].includes(key)) return () => true;
    if (key === 'checkFramebufferStatus') return () => 1;
    return () => ({});
  }});
  canvas.getContext = () => gl;
  canvas.requestPointerLock = () => {
    if (captureFails === 'throw') throw Error('unavailable');
    if (captureFails) return Promise.reject(Error('denied'));
    doc.pointerLockElement = canvas; doc.emit('pointerlockchange'); return Promise.resolve();
  };
  doc.exitPointerLock = () => { doc.pointerLockElement = null; doc.emit('pointerlockchange'); };
  let peer, callbacks;
  const context = {document: doc, window: win, MutationObserver, console, performance, location: {search: ''}, devicePixelRatio: 1,
    matchMedia: () => ({matches: true}), requestAnimationFrame: fn => { nextFrame = fn; },
    fetch: async () => ({ok: true, text: async () => 'shader'}),
    loadWasm: async () => ({instance: {exports: engine}}), showLoadError: error => errors.push(error), configureVisuals() {}, bindVisualPreferences,
    bindInput: (canvas, game, exports, playing) => bindInput(canvas, game, exports, playing, {window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa}),
    bindAudio: () => ({sync() {}, mute() {}, enable: () => Promise.resolve()}),
    bindDetails: () => {
      // The real shared player currently overwrites both attributes at bind.
      canvas.setAttribute('aria-label', 'Owned Lab game canvas');
      canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
      return {ready() {}, update() {}};
    },
    framebufferSize: () => ({width: 960, height: 540, scale: 1}), createInputOverlay: () => () => {},
    GpuTimer: class { begin() {} end() {} },
    PeerLink: class {
      constructor(value) { peer = this; callbacks = value; this.mode = 0; this.sent = 0; }
      close() { this.mode = 0; callbacks.disconnected(); }
      send() { this.sent++; }
    },
  };
  runInNewContext(script, context);
  await runInNewContext(`(async()=>{${platform.replace(/^import .*;$/gm, '')}\n})()`, context);
  flush();
  function frame() { nextFrame(now += 64); flush(); assert.deepEqual(errors, []); }
  function key(type, code, target = canvas, extra = {}) {
    const event = {key: code, code, target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; },
      stopImmediatePropagation() { this.stopped = this.immediate = true; }, ...extra};
    win.emit(type, event, true);
    if (!event.stopped) target.emit(type, event);
    if (!event.stopped) doc.emit(type, event);
    if (!event.stopped) win.emit(type, event);
    return event;
  }
  return {$, doc, win, canvas, calls, frames, key, flush, frame, peer,
    setPanel: value => { panel = value; }, requestMenu: value => { requestedMenu = value; },
    failCapture: (value = true) => { captureFails = value; }, setFocused: value => { focused = value; },
    setDialog: value => { dialog = value; }, connect: () => { peer.mode = 1; callbacks.connected(1); }, mode: () => mode};
}

test('shared player descriptions retain the real title and settle without duplicate IDs', async () => {
  assert.match(read('../player.js'), /canvas\.setAttribute\('aria-describedby',help\.id\)/);
  const f = await fixture();
  assert.equal(f.doc.activeElement, f.doc.body, 'boot must not steal focus');
  for (const value of ['ferravex-canvas-help', 'other-help other-help', '']) {
    f.canvas.setAttribute('aria-describedby', value);
    f.canvas.setAttribute('aria-label', 'Owned Lab game canvas');
    f.flush();
    assert.equal(f.canvas.getAttribute('aria-label'), 'Courtyard game canvas');
    const ids = f.canvas.getAttribute('aria-describedby').split(' ');
    for (const id of [...descriptionIds, ...value.split(' ').filter(Boolean)]) assert.ok(ids.includes(id));
    assert.equal(ids.length, new Set(ids).size);
    f.flush();
  }
});

test('Tab and DOM activation/scrolling do not leak into gameplay; held key releases still reach the bridge', async () => {
  const f = await fixture(); f.calls.length = 0;
  const link = new (f.canvas.constructor)('a');
  for (const target of [f.canvas, f.$('play'), f.$('shader'), link]) {
    for (const type of ['keydown', 'keyup']) for (const shiftKey of [false, true]) assert.equal(f.key(type, 'Tab', target, {shiftKey}).defaultPrevented, false);
  }
  for (const target of [f.$('play'), f.$('shader'), link]) for (const key of ['Enter', 'Space', 'ArrowDown', 'KeyR']) assert.equal(f.key('keydown', key, target).defaultPrevented, false);
  assert.deepEqual(f.calls, []);
  for (const [code, numeric] of [['Escape', 0xff1b], ['F2', 0xffbf], ['Enter', 0xff0d], ['KeyW', 119], ['KeyR', 114], ['Space', 32], ['ArrowLeft', 0xff51]]) {
    assert.equal(f.key('keydown', code).defaultPrevented, true);
    f.key('keyup', code, f.$('shader'));
    assert.deepEqual(f.calls.slice(-2), [['key', numeric, 1], ['key', numeric, 0]]);
  }
  const before = f.calls.length;
  f.key('keydown', 'KeyR', f.canvas, {ctrlKey: true});
  assert.equal(f.calls.length, before);
});

test('Scene tools opens with focus, Escape closes through the existing handler and reset restores the menu', async () => {
  const f = await fixture();
  f.$('edit').focus(); f.$('edit').click(); f.flush();
  assert.equal(f.doc.activeElement, f.$('closeTools'));
  assert.equal(f.$('edit').getAttribute('aria-expanded'), 'true');
  f.$('shader').focus(); f.calls.length = 0;
  assert.equal(f.key('keydown', 'Escape', f.$('shader')).defaultPrevented, true); f.flush();
  assert.equal(f.$('tools').hidden, true);
  assert.equal(f.$('menu').hidden, false);
  assert.equal(f.doc.activeElement, f.$('edit'));
  assert.equal(f.$('edit').getAttribute('aria-expanded'), 'false');
  assert.deepEqual(f.calls, [], 'closing tools neither confirms play nor sends game Escape');
  f.$('edit').click(); f.flush(); f.$('reset').focus(); f.$('reset').click(); f.flush();
  assert.equal(f.doc.activeElement, f.$('edit'));
  assert.equal(f.$('menu').hidden, false);
});

test('focus navigation and menu return release local input without stopping authoritative peer frames', async () => {
  const f = await fixture(); f.connect();
  f.$('overview').click(); f.frame();
  assert.equal(f.frames.at(-1)[1], 1);
  f.$('controls').focus(); f.flush(); f.frame();
  assert.equal(f.doc.activeElement, f.$('controls'), 'do not steal toolbar focus');
  assert.equal(f.frames.at(-1)[1], 0);
  assert.equal(f.mode(), 1);
  assert.ok(f.calls.some(call => call[0] === 'release'));
  assert.match(f.$('menu-status').textContent, /Match stays live/);
  const sent = f.peer.sent;
  f.frame(); assert.ok(f.peer.sent > sent, 'peer sends continue under the DOM menu');
  f.$('overview').click(); f.frame(); f.requestMenu(1); f.frame();
  assert.equal(f.doc.activeElement, f.$('play'), 'canvas menu action focuses the resume button');
  f.frame(); assert.equal(f.frames.at(-1)[1], 0);
});

test('Controls close restores menu focus and requires explicit resume; capture failure has an accessible recovery', async () => {
  const f = await fixture();
  f.$('overview').click(); f.frame(); f.$('controls').click(); f.frame();
  assert.equal(f.doc.activeElement, f.canvas, 'Controls keeps canvas navigation ownership');
  f.setPanel(false); f.frame();
  assert.equal(f.doc.activeElement, f.$('play'));
  assert.equal(f.frames.at(-1)[1], 0);
  f.failCapture(); f.$('play').click(); await Promise.resolve(); f.flush(); f.frame();
  assert.match(f.$('menu-status').textContent, /capture failed/);
  assert.equal(f.doc.activeElement, f.$('play'));
  assert.equal(f.frames.at(-1)[1], 0);
  f.$('overview').click(); f.frame(); assert.equal(f.frames.at(-1)[1], 1);
});

test('fullscreen Escape, modal dialogs and background focus keep priority over local tools', async () => {
  const f = await fixture();
  f.$('edit').focus(); f.$('edit').click(); f.flush();
  let escaped = 0;
  f.win.addEventListener('keydown', event => { if (event.key === 'Escape') { escaped++; event.preventDefault(); event.stopImmediatePropagation(); } }, true);
  f.key('keydown', 'Escape', f.$('shader')); f.flush();
  assert.equal(escaped, 1); assert.equal(f.$('tools').hidden, false);
  f.$('closeTools').click(); f.flush();
  f.$('overview').click(); f.frame();
  f.setFocused(false); f.win.emit('blur'); f.frame();
  assert.equal(f.doc.activeElement, f.canvas, 'background transitions never steal DOM focus');
  f.setFocused(true); f.win.emit('focus'); f.frame();
  assert.equal(f.frames.at(-1)[1], 0, 'window focus return never resumes training');
  f.$('overview').click(); f.frame();
  f.setDialog({}); f.requestMenu(1); f.frame();
  assert.equal(f.doc.activeElement, f.canvas, 'a modal owns focus restoration');
});

test('synchronous capture failure and Pause button return focus even when the focused control is hidden', async () => {
  const f = await fixture();
  f.failCapture('throw'); f.$('play').focus(); f.$('play').click(); f.flush(); f.frame();
  assert.match(f.$('menu-status').textContent, /capture unavailable/);
  assert.equal(f.doc.activeElement, f.$('play'));
  assert.equal(f.frames.at(-1)[1], 0);
  f.$('overview').click(); f.frame();
  f.$('pause').focus(); f.$('pause').click(); f.flush(); f.frame();
  assert.equal(f.$('pause').hidden, true);
  assert.equal(f.doc.activeElement, f.$('play'));
  assert.equal(f.frames.at(-1)[1], 0);
});
