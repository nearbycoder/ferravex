// Source/structure and event-contract checks only. No staged Wasm, browser
// layout, screen reader, pointer-lock permission or physical-device claims.
import assert from 'node:assert/strict';
import {readFileSync, statSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';
import {bindInput} from '../input.js';
import {bindPlayer} from '../player.js';

const page = new URL('../strike/index.html', import.meta.url);
const html = readFileSync(page, 'utf8');
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const text = value => value.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
const attributes = source => Object.fromEntries([...source.matchAll(/([\w-]+)(?:="([^"]*)")?/g)].map(([, name, value]) => [name, value ?? '']));
const tags = [...html.matchAll(/<([a-z][\w-]*)\b([^>]*)>/g)].map(([, tag, attrs]) => ({tag, attrs: attributes(attrs)}));
const byId = id => {
  const matches = tags.filter(node => node.attrs.id === id);
  assert.equal(matches.length, 1, `one element with id ${id}`);
  return matches[0];
};
const content = id => {
  const {tag} = byId(id);
  return html.match(new RegExp(`<${tag}\\b[^>]*id="${id}"[^>]*>([\\s\\S]*?)<\\/${tag}>`))[1];
};
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)].map(([, attrs, source]) => ({attrs: attributes(attrs), source}));
const hostScript = scripts.find(script => !script.attrs.src).source;
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];

test('catalog, source routes and the shared runtime remain the boot contract', () => {
  const game = JSON.parse(read('../../examples/catalog.json')).find(game => game.id === 'strike');
  assert.equal(game.host, 'strike');
  assert.equal(byId('strike-canvas').attrs['data-game'], game.id);
  assert.equal(byId('strike-home').attrs.href, '../');
  assert.ok(statSync(new URL('../index.html', page)).isFile());
  assert.deepEqual(scripts.filter(script => script.attrs.src).map(script => script.attrs), [{type: 'module', src: '../runtime.js'}]);
  assert.equal(scripts.length, 2, 'only local host behavior and the original runtime');
  const runtime = read('../runtime.js');
  for (const [, dependency] of runtime.matchAll(/from\s+['"](\.\.?\/[^'"]+)['"]/g)) {
    assert.ok(statSync(new URL(dependency, new URL('../runtime.js', import.meta.url))).isFile(), dependency);
  }
  assert.ok(runtime.includes("loadWasm(`${game.replaceAll('-','_')}.wasm`,imports)"), 'Strike still resolves strike.wasm relative to its page');
  for (const hook of ['ferravex_set_playing', 'ferravex_init', 'ferravex_frame', 'bindInput', 'bindDetails', 'bindAudio', 'bindGraphicsLifecycle', 'RetainedRenderer', 'SpriteRenderer']) {
    assert.ok(runtime.includes(hook), `shared runtime retains ${hook}`);
  }
  assert.doesNotMatch(hostScript, /\b(?:fetch|requestAnimationFrame|setInterval)\s*\(|\bWebAssembly\b|ferravex_\w+\s*\(/, 'host does not replace loading, gameplay or renderer hooks');
  for (const attribute of ['data-controls', 'data-audio', 'data-multiplayer']) {
    const buttons = tags.filter(node => Object.hasOwn(node.attrs, attribute));
    assert.equal(buttons.length, 1, attribute);
    assert.equal(buttons[0].tag, 'button');
    assert.equal(buttons[0].attrs.type, 'button');
    assert.ok(Object.hasOwn(buttons[0].attrs, 'disabled'), 'runtime owns readiness');
  }
  assert.equal(tags.filter(node => node.tag === 'button').length, 3, 'Details, Fullscreen and player tools are inserted by the shared modules');
  assert.match(content('strike-navigation'), /<div class="strike-tools"[^>]*>[\s\S]*data-controls[\s\S]*<\/div>/, 'shared buttons can join the wrapping tools group');
});

test('canvas description, fallback and loading feedback reference real accessible content', () => {
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>Strike · Ferravex<\/title>/);
  assert.equal(tags.filter(node => node.tag === 'main').length, 1);
  assert.equal(tags.filter(node => node.tag === 'h1').length, 1);
  const canvas = byId('strike-canvas').attrs;
  assert.equal(canvas.tabindex, '0');
  assert.match(canvas['aria-label'], /Strike.*Dockyard/);
  assert.ok(text(content('strike-canvas')).length > 40, 'useful canvas fallback text');
  const ids = tags.filter(node => node.attrs.id).map(node => node.attrs.id);
  assert.equal(ids.length, new Set(ids).size, 'no duplicate IDs');
  for (const id of canvas['aria-describedby'].split(/\s+/)) {
    assert.ok(text(content(id)).length > 40, `${id} has explanatory text`);
    assert.ok(!Object.hasOwn(byId(id).attrs, 'aria-hidden'));
  }
  assert.equal(byId('strike-focus-game').attrs.href, '#strike-canvas');
  assert.equal(byId('strike-home').attrs['aria-keyshortcuts'], 'Alt+M');
  assert.match(text(content('strike-navigation-help')), /Tab.*arrow keys.*Enter.*Escape.*Alt\+M.*does not pause/);
  assert.match(html, /<noscript>[\s\S]*JavaScript and WebGL2/);
  const statuses = tags.filter(node => node.attrs.role === 'status');
  assert.equal(statuses.length, 1, 'runtime querySelector selects the loading message, not capture guidance');
  assert.equal(statuses[0].attrs.id, 'strike-loading');
  for (const id of ['strike-help', 'strike-loading']) {
    assert.equal(byId(id).attrs['aria-live'], 'polite');
    assert.equal(byId(id).attrs['aria-atomic'], 'true');
  }
});

test('guidance distinguishes defaults, remapping, browser release and authoritative matches', () => {
  const remapping = text(content('strike-remapping'));
  assert.match(remapping, /keys are defaults.*Saved or remapped controls may differ.*Controls \(F2\)/);
  assert.match(remapping, /current Menu binding.*default is Escape.*browser still uses Escape/);
  assert.match(html, /Offline practice pauses in the menu\. Multiplayer matches continue, and you remain vulnerable/);
  assert.match(html, /Details contains the shared player, display and accessibility settings/);
  assert.doesNotMatch(html, /use the left movement stick|swipe the right side|use touch\/gamepad controls/i, 'no unconditional touch or alternative-device promise');
  assert.doesNotMatch(hostScript, /localStorage|ferravex_controls_load/, 'the host does not decode or cache remapped bindings');
  // Source evidence, not execution of Rust: migration owners should revisit
  // these labels if they change the preset, menu names or additional actions.
  const controls = read('../../examples/strike/src/controls.rs');
  const preset = read('../../crates/ferravex-render/src/input/presets.rs');
  const menu = read('../../examples/strike/src/hud.rs');
  assert.match(controls, /PlayerInput::preset\("strike", Preset::Shooter\)/);
  assert.match(controls, /\("roster", "Players", KeyCode::Tab\)/);
  for (const key of ['W', 'A', 'S', 'D', 'Up', 'Down', 'Left', 'Right', 'Space', 'R', 'Escape']) {
    assert.match(preset, new RegExp(`KeyCode::${key}\\b`));
  }
  assert.match(preset, /InputCode::Mouse\(1\)/);
  for (const label of ['Start practice', 'Resume practice', 'Return to match', 'Return to replay']) assert.ok(menu.includes(`"${label}"`), label);
  assert.match(read('../../crates/ferravex-render/src/ui/shared.rs'), /Keyboard Tab\/arrows\/Enter/);
});

test('source CSS keeps focus and narrow controls readable without covering gameplay', () => {
  assert.match(css, /#strike-navigation\{[^}]*flex-wrap:wrap/);
  assert.match(css, /\.strike-tools\{[^}]*flex-wrap:wrap/);
  assert.match(css, /#strike-navigation button\{[^}]*font:inherit[^}]*min-height:44px/);
  assert.match(css, /#strike-navigation :focus-visible,#strike-guide :focus-visible\{[^}]*outline:3px/);
  assert.match(css, /#strike-canvas:focus-visible\{[^}]*outline:3px[^}]*outline-offset:-3px/);
  assert.match(css, /#strike-guide\{[^}]*overflow-wrap:anywhere/);
  assert.match(css, /grid-template-columns:repeat\(auto-fit,minmax\(min\(100%,15rem\),1fr\)\)/);
  assert.match(css, /body\.ferravex-fullscreen>main\{[^}]*min-height:0[^}]*overflow:auto/);
  assert.doesNotMatch(css, /outline:none|font-size:12px|#strike-help\{[^}]*position:(?:fixed|absolute)|^\[role=status\]/m);
  assert.match(html, /width=device-width,initial-scale=1/);
  assert.doesNotMatch(html, /user-scalable=no|maximum-scale=1/);
});

// A bounded DOM/event double runs the actual inline host controller together
// with bindInput. Native tab traversal, layout and announcements need a browser.
function fixture({bridge = true} = {}) {
  const observers = [], pending = new Set(), calls = [];
  let doc;
  class Element {
    constructor(tag = '', attrs = {}, parent = null) {
      this.tagName = tag;
      this.attrs = {...attrs};
      this.parent = parent;
      this.events = new Map();
      this.hidden = false;
      this.textWrites = 0;
      this.attributeWrites = 0;
      this._text = '';
    }
    get textContent() { return this._text; }
    set textContent(value) { this._text = value; this.textWrites++; }
    getAttribute(name) { return this.attrs[name] ?? null; }
    setAttribute(name, value) {
      this.attrs[name] = String(value);
      this.attributeWrites++;
      for (const observer of observers) if (observer.node === this && observer.filter.includes(name)) pending.add(observer);
    }
    addEventListener(type, callback) {
      if (!this.events.has(type)) this.events.set(type, []);
      this.events.get(type).push(callback);
    }
    removeEventListener(type, callback) {
      this.events.set(type, (this.events.get(type) ?? []).filter(fn => fn !== callback));
    }
    emit(type, properties = {}) {
      const event = {target: this, defaultPrevented: false, stopped: false,
        preventDefault() { this.defaultPrevented = true; },
        stopPropagation() { this.stopped = true; }, ...properties};
      for (let node = this; node && !event.stopped; node = node.parent) {
        for (const listener of node.events.get(type) ?? []) listener(event);
      }
      return event;
    }
    focus() { doc.activeElement = this; }
    closest(selector) { return selector.split(',').includes(this.tagName) ? this : this.parent?.closest(selector) ?? null; }
    getBoundingClientRect() { return {left: 0, top: 0}; }
    setPointerCapture() {}
  }
  const win = new Element();
  doc = new Element('', {}, win);
  doc.hidden = false;
  doc.hasFocus = () => true;
  doc.pointerLockElement = null;
  const nodes = {};
  for (const id of ['strike-navigation', 'strike-guide', 'strike-canvas', 'strike-help', 'strike-home', 'strike-focus-game']) {
    const {tag, attrs} = byId(id);
    nodes[id] = new Element(tag, attrs, doc);
  }
  const nav = nodes['strike-navigation'], guide = nodes['strike-guide'], canvas = nodes['strike-canvas'];
  const home = nodes['strike-home'], focusLink = nodes['strike-focus-game'], help = nodes['strike-help'];
  home.parent = nav;
  focusLink.parent = guide;
  help.parent = guide;
  help.textContent = text(content('strike-help'));
  const summary = new Element('summary', {}, guide);
  const controls = new Element('button', {'data-controls': ''}, nav);
  doc.querySelector = selector => nodes[selector.slice(1)] ?? null;
  const MutationObserver = class {
    constructor(callback) { this.callback = callback; }
    observe(node, options) { this.node = node; this.filter = options.attributeFilter; observers.push(this); }
  };
  runInNewContext(hostScript, {document: doc, window: win, MutationObserver}, {timeout: 1000});
  const flush = () => {
    for (let count = 0; pending.size; count++) {
      assert.ok(count < 5, 'description observer converges without an attribute loop');
      const batch = [...pending]; pending.clear();
      for (const observer of batch) observer.callback();
    }
  };
  const exports = {ferravex_grab: () => 0, ferravex_touch_mouse: () => 0};
  for (const name of ['key', 'blur', 'focus', 'pointer', 'button', 'touch', 'touch_pointer', 'touch_button']) {
    exports[`ferravex_${name}`] = (...args) => calls.push([name, ...args]);
  }
  if (bridge) bindInput(canvas, 'strike', () => exports, () => true, {
    window: win, document: doc, navigator: {getGamepads: () => []}, now: () => 1000,
    storage: () => ({getItem: () => null}), atob, btoa,
  });
  canvas.focus();
  return {doc, win, canvas, help, home, focusLink, summary, controls, flush, calls,
    key: (target, code, extra = {}) => target.emit('keydown', {code, ...extra}),
    lock: element => { doc.pointerLockElement = element; doc.emit('pointerlockchange'); },
  };
}


// Run the real player binder with a minimal DOM. Globals are restored even when
// an assertion fails; no browser defaults, layout or Wasm are simulated here.
function withPlayer(h, game, check) {
  class Node extends EventTarget {
    children = []; attrs = {}; style = {}; dataset = {};
    classList = {toggle() {}};
    constructor(tag) { super(); this.tagName = tag; }
    append(...nodes) { this.children.push(...nodes); }
    insertBefore(node, before) {
      const index = this.children.indexOf(before);
      assert.notEqual(index, -1);
      this.children.splice(index, 0, node);
    }
    replaceChildren(...nodes) { this.children = [...nodes]; }
    contains(node) { return this === node || this.children.some(child => child.contains(node)); }
    setAttribute(name, value) { this.attrs[name] = String(value); }
    getAttribute(name) { return this.attrs[name] ?? null; }
    get value() { return this._value ?? (this.tagName === 'select' ? this.children[0]?.value : '') ?? ''; }
    set value(value) { this._value = String(value); }
  }
  h.doc.createElement = tag => new Node(tag);
  h.doc.head = new Node('head');
  h.doc.body = new Node('body');
  const body = new Node('div');
  h.doc.body.append(body);
  const globals = {
    document: h.doc, window: h.win, navigator: {onLine: true},
    matchMedia: () => Object.assign(new EventTarget(), {matches: false}),
    localStorage: {getItem: () => null},
  };
  const saved = Object.fromEntries(Object.keys(globals).map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  let player;
  try {
    for (const [key, value] of Object.entries(globals)) Object.defineProperty(globalThis, key, {configurable: true, value});
    player = bindPlayer(h.canvas, game, () => undefined, body, () => false);
    check(body, player);
  } finally {
    try { player?.dispose(); }
    finally {
      for (const [key, descriptor] of Object.entries(saved)) {
        if (descriptor) Object.defineProperty(globalThis, key, descriptor);
        else delete globalThis[key];
      }
    }
  }
}

test('host descriptions survive real bindPlayer initialization and reconcile idempotently', () => {
  const h = fixture();
  const focus = {}; h.doc.activeElement = focus;
  withPlayer(h, 'strike', (body, player) => {
    h.flush();
    const hostIds = ['strike-description', 'strike-navigation-help', 'strike-remapping'];
    const sharedHelp = body.children.find(node => node.id === 'ferravex-canvas-help');
    assert.ok(sharedHelp, 'real binder appended its shared help');
    assert.match(sharedHelp.textContent, /Controls.*Details/);
    assert.deepEqual(h.canvas.getAttribute('aria-describedby').split(' '), [...hostIds, sharedHelp.id]);
    assert.deepEqual(player.presentationPreferences, {reducedMotion: 'system', reducedFlash: false});
    for (const current of [
      ` extra-help  ${sharedHelp.id} extra-help ${hostIds[0]} `,
      [...hostIds, 'extra-help', sharedHelp.id].join(' '),
    ]) {
      h.canvas.setAttribute('aria-describedby', current);
      h.flush();
      assert.deepEqual(h.canvas.getAttribute('aria-describedby').split(' '), [...hostIds, 'extra-help', sharedHelp.id]);
      const writes = h.canvas.attributeWrites;
      h.flush();
      assert.equal(h.canvas.attributeWrites, writes, 'settled reconciliation performs no writes');
      // Repeating the same external write queues a real mutation, but the host
      // must not write it back or enter a mutation-observer loop.
      h.canvas.setAttribute('aria-describedby', h.canvas.getAttribute('aria-describedby'));
      h.flush();
      assert.equal(h.canvas.attributeWrites, writes + 1, 'already normalized value needs no host write');
    }
    assert.equal(h.doc.activeElement, focus, 'player boot and reconciliation preserve existing page focus');
  });
});

test('page navigation stays native while canvas Tab, Enter and F2 still reach the shared bridge', () => {
  const h = fixture();
  for (const [target, code] of [[h.home, 'Tab'], [h.home, 'Enter'], [h.summary, 'Space'], [h.summary, 'Enter'], [h.controls, 'Tab'], [h.controls, 'Enter']]) {
    const event = h.key(target, code);
    assert.equal(event.defaultPrevented, false, `${target.tagName} ${code} retains its browser default`);
    assert.equal(target.emit('keyup', {code}).defaultPrevented, false, `${target.tagName} ${code} release stays native`);
  }
  assert.deepEqual(h.calls, [], 'page keyboard interaction cannot start, pause or remap the game');
  for (const [code, number] of [['Tab', 0xff09], ['Enter', 0xff0d], ['F2', 0xffbf]]) {
    assert.equal(h.key(h.canvas, code).defaultPrevented, true);
    assert.deepEqual(h.calls.at(-1), ['key', number, 1]);
    h.canvas.emit('keyup', {code});
    assert.deepEqual(h.calls.at(-1), ['key', number, 0]);
  }
});

test('help disclosure retains the full native Space down/up sequence', () => {
  const h = fixture();
  h.summary.focus();
  for (let press = 0; press < 2; press++) {
    assert.equal(h.key(h.summary, 'Space', {key: ' '}).defaultPrevented, false);
    assert.equal(h.key(h.summary, 'Space', {key: ' ', repeat: true}).defaultPrevented, false);
    assert.equal(h.summary.emit('keyup', {code: 'Space', key: ' '}).defaultPrevented, false);
  }
  assert.deepEqual(h.calls, [], 'page disclosure sends neither a game press nor an orphan release');
});

test('Space held in the game releases on help, then its next activation stays native', () => {
  const h = fixture();
  for (const target of [h.summary, h.home, h.controls]) {
    h.canvas.focus();
    assert.equal(h.key(h.canvas, 'Space').defaultPrevented, true);
    target.focus();
    h.key(target, 'Space', {repeat: true});
    target.emit('keyup', {code: 'Space'});
    assert.deepEqual(h.calls.slice(-2), [['key', 32, 1], ['key', 32, 0]]);
    const count = h.calls.length;
    assert.equal(h.key(h.summary, 'Space').defaultPrevented, false);
    assert.equal(h.summary.emit('keyup', {code: 'Space'}).defaultPrevented, false);
    assert.equal(h.calls.length, count, 'fresh disclosure activation cannot release gameplay again');
  }
  // preventDefault alone does not stop propagation: the real input bridge still
  // accepts this canvas press, so the host must retain its release ownership.
  h.canvas.focus();
  h.key(h.canvas, 'Space', {defaultPrevented: true});
  h.summary.focus();
  h.summary.emit('keyup', {code: 'Space'});
  assert.deepEqual(h.calls.slice(-2), [['key', 32, 1], ['key', 32, 0]]);
});

test('input resets clear game key ownership before a page-owned release', () => {
  for (const transition of ['blur', 'pagehide', 'resize', 'orientationchange', 'visibilitychange', 'pointerlockchange']) {
    const h = fixture();
    h.key(h.canvas, 'Space');
    if (transition === 'visibilitychange') { h.doc.hidden = true; h.doc.emit(transition); }
    else if (transition === 'pointerlockchange') h.lock(h.canvas);
    else h.win.emit(transition);
    assert.ok(h.calls.some(call => call[0] === 'blur'), `${transition} resets real input`);
    const count = h.calls.length;
    assert.equal(h.summary.emit('keyup', {code: 'Space'}).defaultPrevented, false, transition);
    assert.equal(h.calls.length, count, 'already-reset gameplay needs no extra release');
  }
});

test('shortcuts, composition and repeats cannot claim page Space releases', () => {
  for (const extra of [{ctrlKey: true}, {metaKey: true}, {altKey: true}, {isComposing: true}, {keyCode: 229}, {repeat: true}]) {
    const h = fixture();
    h.key(h.canvas, 'Space', extra);
    assert.equal(h.summary.emit('keyup', {code: 'Space'}).defaultPrevented, false);
    assert.deepEqual(h.calls, [], 'bridge did not accept a gameplay press');
  }
});

test('bounded tracking overflow still releases gameplay and clears at lifecycle reset', () => {
  const h = fixture();
  for (let i = 0; i < 200; i++) h.key(h.canvas, `Synthetic${i}`);
  h.key(h.canvas, 'Space');
  h.summary.emit('keyup', {code: 'Space'});
  assert.deepEqual(h.calls, [['key', 32, 1], ['key', 32, 0]], 'overflow cannot strand accepted gameplay');
  h.win.emit('blur');
  h.win.emit('focus');
  const count = h.calls.length;
  h.key(h.summary, 'Space');
  assert.equal(h.summary.emit('keyup', {code: 'Space'}).defaultPrevented, false);
  assert.equal(h.calls.length, count, 'reset restores native disclosure activation');
});

test('Alt+M escapes canvas focus after release and held gameplay keys can still release', () => {
  const h = fixture();
  h.key(h.canvas, 'KeyW');
  const event = h.key(h.canvas, 'KeyM', {altKey: true});
  assert.equal(event.defaultPrevented, true);
  assert.equal(h.doc.activeElement, h.home);
  assert.deepEqual(h.calls, [['key', 119, 1]], 'navigation introduces no gameplay or lifecycle calls');
  h.home.emit('keyup', {code: 'KeyW'});
  assert.deepEqual(h.calls.at(-1), ['key', 119, 0], 'release reaches the bridge after focus leaves the canvas');
  assert.equal(h.focusLink.emit('click').defaultPrevented, true);
  assert.equal(h.doc.activeElement, h.canvas, 'explicit focus link returns to the view without requesting capture');
});

test('focus shortcut leaves locked play, modified shortcuts, composition and repeats alone', () => {
  const h = fixture();
  for (const extra of [{}, {altKey: true, ctrlKey: true}, {altKey: true, metaKey: true}, {altKey: true, shiftKey: true}, {altKey: true, repeat: true}, {altKey: true, isComposing: true}, {altKey: true, keyCode: 229}]) {
    h.key(h.canvas, 'KeyM', extra);
    assert.equal(h.doc.activeElement, h.canvas);
  }
  h.lock(h.canvas);
  const before = h.calls.length;
  assert.equal(h.key(h.canvas, 'KeyM', {altKey: true}).defaultPrevented, false);
  assert.equal(h.doc.activeElement, h.canvas);
  assert.equal(h.doc.pointerLockElement, h.canvas);
  assert.equal(h.calls.length, before);
});

test('capture guidance preserves shared Escape deduplication and never steals focus', () => {
  const h = fixture();
  h.lock(h.canvas);
  assert.match(h.help.textContent, /Mouse captured.*Menu binding.*Controls \(F2\)/);
  h.key(h.canvas, 'Escape');
  h.canvas.emit('keyup', {code: 'Escape'});
  h.lock(null);
  assert.deepEqual(h.calls.filter(call => call[0] === 'key'), [['key', 0xff1b, 1], ['key', 0xff1b, 0]], 'one physical Escape is not toggled twice');
  assert.match(h.help.textContent, /Mouse released.*Menu binding.*Multiplayer matches continue/);
  assert.equal(h.doc.activeElement, h.canvas);
  const writes = h.help.textWrites;
  h.doc.emit('pointerlockchange');
  assert.equal(h.help.textWrites, writes, 'unchanged capture state is not announced repeatedly');
});

test('capture failure remains useful after touch, and unrelated locks do not announce success', () => {
  const h = fixture({bridge: false});
  const initial = h.help.textContent;
  h.lock({});
  h.doc.emit('pointerlockerror');
  assert.equal(h.help.textContent, initial);
  h.lock(null);
  h.canvas.emit('pointerdown', {pointerType: 'touch'});
  h.doc.emit('pointerlockerror');
  assert.equal(h.help.hidden, false, 'touch does not permanently suppress mouse recovery help');
  assert.match(h.help.textContent, /capture was blocked.*start or resume.*click the game view.*Controls \(F2\)/);
  assert.doesNotMatch(h.help.textContent, /use touch|gamepad controls/i);
  const writes = h.help.textWrites;
  h.doc.emit('pointerlockerror');
  assert.equal(h.help.textWrites, writes);
  h.lock(h.canvas);
  assert.match(h.help.textContent, /Mouse captured/);
  assert.equal(h.doc.activeElement, h.canvas);
});
