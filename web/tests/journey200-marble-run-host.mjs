// Source/structure and real bindInput boundary checks. No staged Wasm, browser
// layout, native focus traversal, screen reader or gameplay claims are made here.
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';
import {bindPlayer} from '../player.js';

const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const html = read('../marble-run/index.html');
const runtime = read('../runtime.js');
const catalog = JSON.parse(read('../../examples/catalog.json'));
const attrs = source => Object.fromEntries([...source.matchAll(/([\w-]+)(?:="([^"]*)")?/g)]
  .map(([, name, value]) => [name, value ?? '']));
const tags = [...html.matchAll(/<([a-z][\w-]*)\b([^>]*)>/g)]
  .map(match => ({name: match[1], attrs: attrs(match[2]), index: match.index}));
const all = name => tags.filter(tag => tag.name === name);
const byId = id => tags.find(tag => tag.attrs.id === id);
const content = tag => html.slice(html.indexOf('>', tag.index) + 1, html.indexOf(`</${tag.name}>`, tag.index));
const plain = source => source.replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim();
let passed = 0;
function test(name, run) {
  run();
  passed++;
  console.log('ok - ' + name);
}

test('named canvas has visible, resolvable objective and keyboard-exit descriptions', () => {
  assert.equal(all('html')[0].attrs.lang, 'en');
  assert.equal(content(all('title')[0]), 'Marble Run · Ferravex');
  assert.equal(all('h1').length, 1);
  assert.equal(content(all('h1')[0]), 'Marble Run');
  assert.equal(all('canvas').length, 1);
  const canvas = all('canvas')[0];
  assert.equal(canvas.attrs.tabindex, '0');
  assert.match(canvas.attrs['aria-label'], /Marble Run.*3D.*obstacle course/);
  assert.match(content(canvas), /canvas and WebGL2/);
  assert.notEqual(canvas.attrs.role, 'application');
  const descriptions = canvas.attrs['aria-describedby'].split(/\s+/);
  assert.deepEqual(descriptions, ['game-description', 'keyboard-exit']);
  for (const id of descriptions) {
    const target = byId(id);
    assert.ok(target, `description ${id} exists`);
    assert.equal(target.name, 'p');
    assert.ok(!('hidden' in target.attrs));
    assert.notEqual(target.attrs['aria-hidden'], 'true');
    assert.ok(target.index < all('details')[0].index, 'description is outside collapsed help');
    assert.ok(plain(content(target)).length > 50);
  }
  assert.match(plain(content(byId('game-description'))), /four courses.*countdown.*every crystal.*finish arch/);
  assert.match(plain(content(byId('keyboard-exit'))), /Shift\+Tab.*page controls/);
  const ids = tags.flatMap(tag => tag.attrs.id ? [tag.attrs.id] : []);
  assert.equal(new Set(ids).size, ids.length, 'IDs are unique');
  assert.ok(tags.every(tag => !tag.attrs.tabindex || Number(tag.attrs.tabindex) <= 0));
  assert.ok(tags.every(tag => !('autofocus' in tag.attrs)));
});

test('help distinguishes defaults, saved bindings, capture cancellation and course/manual pause', () => {
  const help = all('details')[0];
  assert.equal(byId('game-help').name, 'summary');
  const text = plain(content(help));
  assert.match(text, /default keys and remapping/);
  assert.match(text, /Saved or remapped controls can differ.*current bindings and remap/);
  assert.match(text, /Changes save on this device when storage is available/);
  assert.match(text, /arrow keys or Tab.*Enter.*choose a course/);
  assert.match(text, /Escape cancels a binding capture; otherwise Escape or F2 closes Controls/);
  assert.match(text, /Choose course binding \(default C\)/);
  assert.match(text, /separate manual pause/);
  assert.match(text, /Details → Player tools/);
  assert.match(text, /Done or Escape closes the dialog and returns focus to the game/);
  assert.doesNotMatch(text, /swipe|tap anywhere|touch support/i);

  const expected = [
    ['Roll', /WASD or arrow keys.*camera/], ['Brake', /Hold Space/],
    ['Course overview', /Hold Tab during play/], ['Choose course', /C opens or closes/],
    ['Recover', /V.*after GO while playing.*counts as a fall/],
    ['Restart', /R.*countdown/], ['Pause / resume', /Escape.*if remapped/],
  ];
  assert.equal(all('dt').length, expected.length);
  expected.forEach(([label, description], index) => {
    assert.equal(plain(content(all('dt')[index])), label);
    assert.match(plain(content(all('dd')[index])), description);
  });
});

test('host styles retain focus, wrapping, readable defaults and room for shared tools', () => {
  const css = content(all('style')[0]);
  // These are source guards, not a CSS layout engine or a visual test.
  assert.match(css, /#marble-toolbar\s*\{[^}]*flex-wrap: wrap;[^}]*height: auto;/);
  assert.match(css, /#marble-toolbar button[^}]*min-height: 44px;/);
  assert.match(css, /#marble-toolbar :focus-visible\s*\{[^}]*outline: 3px solid/);
  assert.match(css, /#marble-canvas:focus-visible\s*\{[^}]*outline: 3px solid[^}]*outline-offset: -4px/);
  assert.match(css, /\.marble-host :focus-visible\s*\{[^}]*outline: 3px solid/);
  assert.match(css, /body\.marble-host\s*\{[^}]*overflow: auto;/);
  assert.match(css, /#marble-canvas\s*\{[^}]*flex: 1 0 324px;[^}]*min-height: 324px;[^}]*height: auto;/);
  assert.match(css, /@media \(max-width: 540px\)[\s\S]*grid-template-columns: minmax\(0, 1fr\)/);
  assert.doesNotMatch(css, /outline:\s*(?:none|0)\b|white-space:\s*nowrap/);
  assert.doesNotMatch(all('meta').find(tag => tag.attrs.name === 'viewport').attrs.content, /user-scalable|maximum-scale/);
  const controls = tags.filter(tag => 'data-controls' in tag.attrs);
  const audio = tags.filter(tag => 'data-audio' in tag.attrs);
  for (const group of [controls, audio]) {
    assert.equal(group.length, 1);
    assert.equal(group[0].name, 'button');
    assert.equal(group[0].attrs.type, 'button');
    assert.equal(group[0].attrs['aria-pressed'], 'false');
    assert.ok('disabled' in group[0].attrs, 'shared boot owns enabling');
  }
  assert.equal(all('button').length, 2, 'Details/fullscreen/player tools are supplied once by shared code');
  const status = tags.filter(tag => tag.attrs.role === 'status');
  assert.equal(status.length, 1);
  assert.equal(status[0].attrs.id, 'marble-status');
  assert.equal(status[0].attrs['aria-live'], 'polite');
  assert.equal(status[0].attrs['aria-atomic'], 'true');
  assert.match(css, /#marble-status:empty\s*\{\s*display: none;/);
});

test('catalog, direct/site routes and original runtime boot are preserved', () => {
  const entry = catalog.find(game => game.id === all('canvas')[0].attrs['data-game']);
  assert.equal(entry.id, 'marble-run');
  assert.equal(entry.host, 'marble-run');
  const scripts = all('script').filter(tag => tag.attrs.src);
  assert.equal(scripts.length, 1);
  assert.equal(scripts[0].attrs.type, 'module');
  assert.equal(scripts[0].attrs.src, '../runtime.js');
  for (const base of ['https://local.test/marble-run/', 'https://local.test/play/marble-run/']) {
    const root = new URL('../', base);
    assert.equal(new URL(scripts[0].attrs.src, base).href, new URL('runtime.js', root).href);
    for (const link of all('a')) {
      const resolved = new URL(link.attrs.href, base);
      if (resolved.hash) assert.ok(byId(resolved.hash.slice(1)), 'help route has a real target');
      else assert.equal(resolved.href, root.href, 'All games stays within the current catalog mount');
    }
    assert.equal(new URL(`${entry.id.replaceAll('-', '_')}.wasm`, base).pathname,
      new URL('marble_run.wasm', base).pathname);
  }
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-','_'\)\}\.wasm`,imports\)/);
  for (const file of ['loader', 'input', 'details', 'audio', 'lifecycle', 'retained', 'effects', 'visuals']) {
    assert.ok(runtime.includes(`from './${file}.js'`));
    assert.ok(existsSync(new URL(`../${file}.js`, import.meta.url)));
  }
  assert.match(runtime, /exports\.ferravex_init\(\)/);
  assert.match(runtime, /inputPlatform\.openSettings\(\);canvas\.focus\(\)/);
  const boundary = content(byId('marble-keyboard-boundary'));
  assert.doesNotMatch(boundary, /ferravex_|WebAssembly|fetch\(|requestAnimationFrame|localStorage/,
    'local focus boundary does not replace boot, ABI, storage or simulation');
});

class Target {
  events = new Map();
  constructor(name = '') { this.name = name; }
  addEventListener(type, callback) {
    if (!this.events.has(type)) this.events.set(type, []);
    this.events.get(type).push(callback);
  }
  removeEventListener(type, callback) {
    this.events.set(type, (this.events.get(type) ?? []).filter(fn => fn !== callback));
  }
  emit(type, event) { for (const callback of this.events.get(type) ?? []) callback(event); }
  closest(selector) { return selector.split(',').includes(this.name) ? this : null; }
}
function fixture() {
  const doc = new Target(), win = new Target(), canvas = new Target('canvas'), calls = [];
  doc.getElementById = id => { assert.equal(id, 'marble-canvas'); return canvas; };
  doc.hasFocus = () => true;

  const observers = [], pending = new Set();
  canvas.attrs = {...all('canvas')[0].attrs};
  canvas.attributeWrites = 0;
  canvas.getAttribute = name => canvas.attrs[name] ?? null;
  canvas.setAttribute = (name, value) => {
    canvas.attrs[name] = String(value);
    canvas.attributeWrites++;
    for (const observer of observers) if (observer.node === canvas && observer.filter.includes(name)) pending.add(observer);
  };
  const MutationObserver = class {
    constructor(callback) { this.callback = callback; }
    observe(node, options) { this.node = node; this.filter = options.attributeFilter; observers.push(this); }
  };
  const flush = () => {
    for (let count = 0; pending.size; count++) {
      assert.ok(count < 5, 'description observer converges without an attribute loop');
      const batch = [...pending]; pending.clear();
      for (const observer of batch) observer.callback();
    }
  };

  const exports = {ferravex_key: (...args) => calls.push(args), ferravex_blur() {}, ferravex_focus() {}};
  // Execute the actual inline host listener alongside the real shared bridge.
  const context = {document: doc, window: win, MutationObserver};
  runInNewContext(content(byId('marble-keyboard-boundary')), context);
  bindInput(canvas, 'marble-run', () => exports, () => false, {
    window: win, document: doc, navigator: {},
  });
  const key = (type, code, target = canvas, fields = {}) => {
    const event = {code, target, defaultPrevented: false, stopped: false, ...fields,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; },
    };
    // Model only bubbling/cancellation, not browser default actions or layout.
    target.emit(type, event);
    if (!event.stopped) doc.emit(type, event);
    if (!event.stopped) win.emit(type, event);
    return event;
  };
  return {canvas, calls, key, doc, win, context, flush};
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
  withPlayer(h, 'marble-run', (body, player) => {
    h.flush();
    const hostIds = ['game-description', 'keyboard-exit'];
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

test('plain Tab, pause, F2 and arbitrary remapped keys still reach the game', () => {
  const h = fixture();
  for (const [code, value] of [['Tab', 0xff09], ['Escape', 0xff1b], ['F2', 0xffbf], ['KeyJ', 106], ['ArrowUp', 0xff52], ['Enter', 0xff0d]]) {
    assert.equal(h.key('keydown', code).defaultPrevented, true);
    h.key('keyup', code);
    assert.deepEqual(h.calls.slice(-2), [[value, 1], [value, 0]]);
  }
});

test('Shift+Tab leaves browser default navigation available and releases held game input', () => {
  const h = fixture();
  h.key('keydown', 'KeyW');
  const exit = h.key('keydown', 'Tab', h.canvas, {shiftKey: true});
  assert.equal(exit.stopped, true);
  assert.equal(exit.defaultPrevented, false, 'native backward focus traversal remains available');
  assert.deepEqual(h.calls, [[119, 1]], 'exit is not an overview/menu/gameplay press');
  const button = new Target('button');
  h.key('keyup', 'KeyW', button);
  assert.deepEqual(h.calls.at(-1), [119, 0], 'release survives focus moving to a page button');
  h.key('keydown', 'Tab');
  h.key('keydown', 'Tab', h.canvas, {shiftKey: true});
  h.key('keyup', 'Tab', button);
  assert.deepEqual(h.calls.slice(-2), [[0xff09, 1], [0xff09, 0]], 'held overview cannot get stuck');
});

test('page links and disclosure keep native keydown/keyup defaults without starting gameplay', () => {
  const h = fixture();
  for (const name of ['a', 'summary', 'body']) for (const code of ['Tab', 'Enter', 'Space', 'ArrowDown', 'KeyR']) {
    for (const type of ['keydown', 'keyup']) {
      const event = h.key(type, code, new Target(name));
      assert.equal(event.stopped, true);
      assert.equal(event.defaultPrevented, false);
    }
  }
  assert.deepEqual(h.calls, []);
});

test('release on help cannot strand a held key, while later Space activation remains native', () => {
  const h = fixture(), summary = new Target('summary');
  h.key('keydown', 'Space');
  h.key('keydown', 'Space', summary, {repeat: true});
  h.key('keyup', 'Space', summary);
  assert.deepEqual(h.calls, [[32, 1], [32, 0]]);
  assert.equal(h.key('keydown', 'Space', summary).defaultPrevented, false);
  assert.equal(h.key('keyup', 'Space', summary).defaultPrevented, false);
  assert.deepEqual(h.calls, [[32, 1], [32, 0]], 'help activation never becomes a brake press');
});

test('bounded key bookkeeping clears on blur, pagehide and document hiding', () => {
  for (const transition of ['blur', 'pagehide', 'visibilitychange']) {
    const h = fixture();
    for (let i = 0; i < 200; i++) h.key('keydown', `Synthetic${i}`);
    assert.equal(runInNewContext('marbleGameKeys.size', h.context), 128);
    h.key('keydown', 'KeyW');
    h.key('keyup', 'KeyW', new Target('summary'));
    assert.deepEqual(h.calls, [[119, 1], [119, 0]], 'overflow still releases accepted gameplay keys');
    if (transition === 'visibilitychange') { h.doc.hidden = true; h.doc.emit(transition, {}); }
    else h.win.emit(transition, {});
    assert.equal(runInNewContext('marbleGameKeys.size', h.context), 0);
    assert.equal(runInNewContext('marbleKeyOverflow', h.context), false);
    assert.equal(h.key('keyup', 'Space', new Target('summary')).defaultPrevented, false);
  }
});

test('shared buttons, editors, dialog Escape and browser shortcuts retain ownership', () => {
  const h = fixture();
  for (const name of ['button', 'input', 'textarea', 'select', '[contenteditable]']) {
    const event = h.key('keydown', 'Enter', new Target(name));
    assert.equal(event.stopped, false);
    assert.equal(event.defaultPrevented, false);
  }
  const dialog = new Target('dialog');
  dialog.addEventListener('keydown', event => event.stopPropagation()); // bindDetails contract
  assert.equal(h.key('keydown', 'Escape', dialog).defaultPrevented, false, 'native dialog cancel remains available');
  for (const modifier of ['ctrlKey', 'metaKey', 'altKey']) {
    const event = h.key('keydown', 'Tab', h.canvas, {shiftKey: true, [modifier]: true});
    assert.equal(event.stopped, false);
    assert.equal(event.defaultPrevented, false);
  }
  assert.deepEqual(h.calls, []);
});

console.log(`Marble Run host: ${passed} source/structure and input-boundary checks passed (no visual/Wasm playtest).`);
