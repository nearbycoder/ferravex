// Source/structure and real input-bridge event contracts only. No dist files,
// Wasm builds, browser layout or screen-reader emulation are used by this test.
// Run: node web/tests/journey200-siege-foundry-host.mjs
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';
import {bindPlayer} from '../player.js';

const hostUrl = new URL('../siege-foundry/index.html', import.meta.url);
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const html = readFileSync(hostUrl, 'utf8');
const markup = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/g, '');
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const script = html.match(/<script id="range-navigation">([\s\S]*?)<\/script>/)[1];
const runtime = read('../runtime.js');
const gameplay = read('../../examples/siege-foundry/src/lib.rs');
const presets = read('../../crates/ferravex-render/src/input/presets.rs');
const player = read('../../crates/ferravex-render/src/input/player.rs');

// Inspect this controlled host's source, not arbitrary HTML or a browser DOM.
const text = value => value.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
function attributes(tag) {
  return Object.fromEntries([...tag.matchAll(/([\w:-]+)(?:="([^"]*)")?/g)]
    .slice(1).map(([, name, value]) => [name, value ?? true]));
}
const tags = name => [...markup.matchAll(new RegExp(`<${name}\\b[^>]*>`, 'g'))]
  .map(match => attributes(match[0]));
function element(id) {
  const match = markup.match(new RegExp(`<([\\w-]+)\\b([^>]*\\bid="${id}"[^>]*)>([\\s\\S]*?)<\\/\\1>`));
  assert.ok(match, `#${id} exists`);
  return {attributes: attributes(`<${match[1]}${match[2]}>`), text: text(match[3])};
}

test('canvas has an accurate name, visible descriptions, focus entry and fallback', () => {
  assert.equal(tags('html')[0].lang, 'en');
  assert.match(markup, /<title>Siege Foundry · Ferravex<\/title>/);
  assert.equal(tags('h1').length, 1);
  assert.match(markup, /<h1>Siege Foundry<\/h1>/);
  const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length, 'ARIA and fragment targets are unique');
  assert.equal(tags('canvas').length, 1);
  const canvas = tags('canvas')[0];
  assert.equal(canvas.tabindex, '0');
  assert.equal(canvas['aria-label'], 'Siege Foundry demolition range');
  assert.equal(canvas.role, undefined, 'interactive canvas is not mislabeled as an image or application');
  const descriptions = canvas['aria-describedby'].split(/\s+/);
  assert.deepEqual(descriptions, ['range-objective', 'range-controls', 'range-remapping', 'range-focus']);
  for (const id of descriptions) {
    const node = element(id);
    assert.ok(node.text.length > 60);
    assert.equal(node.attributes.hidden, undefined);
    assert.equal(node.attributes['aria-hidden'], undefined);
    assert.equal(node.attributes.class, undefined, 'guidance is readable page content, not clipped text');
  }
  assert.match(element('range-objective').text, /six rust-red targets with eight cannonballs/);
  assert.match(element('range-remapping').text, /Saved or remapped controls take precedence/);
  assert.match(element('range-focus').text, /moving focus within the page does not pause/);
  assert.match(element('range-focus').text, /Keep Tab for page navigation when remapping/);
  assert.match(element(canvas.id).text, /JavaScript, WebAssembly and WebGL2/);
  assert.equal(tags('noscript').length, 1);
  assert.equal(tags('section')[0]['aria-labelledby'], 'range-help');
  assert.equal(element('range-help').attributes.tabindex, '-1', 'guide jump has a focus destination');
});

test('defaults and game guidance match the current Turret profile and Siege Foundry source', () => {
  const prose = text(markup);
  assert.match(gameplay, /PlayerInput::preset\("siege-foundry", Preset::Turret\)/);
  assert.match(gameplay, /set_mouse_look_button\(Some\(1\)\)/);
  for (const [action, key, arrow] of [
    ['move_left', 'A', 'Left'], ['move_right', 'D', 'Right'],
    ['move_forward', 'W', 'Up'], ['move_back', 'S', 'Down'],
  ]) assert.ok(presets.includes(`("${action}",`)
    && presets.includes(`KeyCode::${key}, KeyCode::${arrow}`), `${action} keyboard defaults`);
  assert.match(presets, /if preset == Preset::Turret\s*\{\s*bindings.push\(key\(KeyCode::Space\)\)/);
  for (const [action, key] of [['restart', 'R'], ['confirm', 'Enter']]) {
    assert.match(presets, new RegExp(`"${action}"[\\s\\S]*?key\\(KeyCode::${key}\\)`));
  }
  assert.match(presets, /let pause_key =[\s\S]*?KeyCode::Escape/);
  for (const label of ['Play / confirm', 'Restart', 'Pause / menu']) assert.ok(presets.includes(`"${label}"`));
  for (const [name, value] of [['AMMO', '8'], ['GOAL', '6'], ['SPEED', '28.0'], ['RELOAD', '0.8'], ['SETTLE', '6.0']]) {
    assert.match(gameplay, new RegExp(`const ${name}: [^=]+ = ${value.replace('.', '\\.')}[;]`));
  }
  assert.match(prose, /With default bindings, Enter or Space starts/);
  assert.match(prose, /Launch power is fixed at 28 metres per second/);
  assert.match(prose, /0.8-second reload.*press during reload is discarded/);
  assert.match(prose, /Starting or retrying after a result does not fire a shot/);
  assert.match(prose, /hold the left button and drag in the scene to aim/);
  assert.match(prose, /six seconds to settle before a loss/);
  assert.match(prose, /Escape cancels a binding capture; Escape again closes the editor/);
  assert.match(player, /key == 0xff1b && self.capture.is_some\(\)/);
  assert.match(player, /self.capture = None/);
  assert.match(prose, /Details → Player tools/);
  assert.match(prose, /Escape may exit fullscreen before reaching the game/);
  assert.doesNotMatch(prose, /\btouch\b|\bswipe\b|\btap\b/i, 'no unsupported device claims');
});

test('shared entry, readiness, runtime hooks and relative routes stay intact', () => {
  const game = JSON.parse(read('../../examples/catalog.json')).find(entry => entry.id === 'siege-foundry');
  assert.equal(game.host, 'siege-foundry');
  assert.equal(game.title, 'Siege Foundry');
  assert.equal(tags('canvas')[0]['data-game'], game.id);
  const scripts = [...html.matchAll(/<script\b[^>]*>/g)].map(match => attributes(match[0]));
  assert.deepEqual(scripts.filter(script => script.src), [{type: 'module', src: '../runtime.js'}]);
  assert.equal(scripts.length, 2, 'one page-navigation guard and the unchanged boot module');
  assert.ok(existsSync(new URL('../runtime.js', hostUrl)));
  for (const module of ['loader', 'retained', 'effects', 'details', 'input', 'audio', 'lifecycle']) {
    assert.ok(runtime.includes(`from './${module}.js'`), `${module} remains runtime-owned`);
  }
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-','_'\)\}\.wasm`,imports\)/);
  assert.match(runtime, /exports\.ferravex_init\(\)/);
  assert.match(runtime, /exports\.ferravex_autoplay\(/);
  assert.match(runtime, /inputPlatform\.openSettings\(\);canvas\.focus\(\)/);
  for (const tool of ['multiplayer', 'replay', 'content', 'material']) {
    assert.ok(runtime.includes(`import('./${tool}-tools.js')`), `${tool} tools remain available`);
  }
  for (const prefix of ['/', '/web/', '/play/', '/nested/play/']) {
    const route = new URL(`${prefix}${game.host}/?autoplay&look=warm`, 'https://example.test');
    assert.equal(new URL('../runtime.js', route).pathname, `${prefix}runtime.js`);
    assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, route).pathname, `${prefix}siege-foundry/siege_foundry.wasm`);
    for (const link of tags('a')) {
      const destination = new URL(link.href, route);
      if (link.href.startsWith('#')) {
        assert.equal(destination.pathname, route.pathname);
        assert.equal(destination.search, route.search);
        element(destination.hash.slice(1));
      } else {
        assert.equal(destination.pathname, prefix);
        assert.ok(existsSync(new URL(`${link.href}index.html`, hostUrl)));
      }
    }
  }
  assert.equal(tags('button').length, 2, 'shared Details, Fullscreen and player tools are not duplicated');
  for (const hook of ['data-controls', 'data-audio']) {
    const buttons = tags('button').filter(button => button[hook] !== undefined);
    assert.equal(buttons.length, 1);
    assert.equal(buttons[0].type, 'button');
    assert.equal(buttons[0].disabled, true);
    assert.equal(buttons[0]['aria-pressed'], 'false');
  }
  const status = element('load-status').attributes;
  assert.equal(status.role, 'status');
  assert.equal(status['aria-live'], 'polite');
  assert.equal(status['aria-atomic'], 'true');
  assert.doesNotMatch(script, /fetch\(|WebAssembly|requestAnimationFrame|setInterval|ferravex_|\.focus\(/,
    'page guard cannot own game lifecycle, ABI, renderer or focus restoration');
});

test('source styles preserve visible focus and wrapping despite shared player styles', () => {
  assert.match(css, /#siege-host :focus-visible\s*\{[^}]*outline: 3px solid/);
  assert.match(css, /#siege-host > canvas:focus-visible\s*\{[^}]*outline-offset: -4px/);
  // ID-scoped selectors outrank the shared Details/Fullscreen body rules even
  // when their styles are appended after this page's styles.
  assert.match(css, /#siege-host > nav\s*\{[^}]*flex-wrap: wrap[^}]*height: auto/);
  assert.match(css, /#siege-host > nav > a\s*\{[^}]*display: block[^}]*white-space: normal/);
  assert.match(css, /#siege-host > nav > button\s*\{[^}]*min-height: 44px[^}]*font: inherit/);
  assert.match(css, /@media \(max-width: 640px\)[\s\S]*\.range-keys\s*\{ grid-template-columns: minmax\(0, 1fr\)/);
  assert.match(css, /#siege-host > canvas\s*\{[^}]*min-height: 28rem/);
  assert.match(css, /#siege-host\.ferravex-fullscreen\s*\{ overflow: auto/);
  assert.doesNotMatch(css, /outline:\s*none|overflow:\s*hidden|\[role[=\s]/,
    'do not hide keyboard focus, clip the guide, or restyle shared live regions');
});

function fixture({loaded = true} = {}) {
  class Target {
    listeners = new Map();
    addEventListener(type, fn, options) {
      const entries = this.listeners.get(type) ?? [];
      entries.push({fn, capture: options === true || !!options?.capture});
      this.listeners.set(type, entries);
    }
    removeEventListener(type, fn) {
      this.listeners.set(type, (this.listeners.get(type) ?? []).filter(entry => entry.fn !== fn));
    }
    emit(type, event, capture = false) {
      for (const entry of this.listeners.get(type) ?? []) if (entry.capture === capture) entry.fn(event);
    }
    closest() { return null; }
  }
  const win = new Target(), doc = new Target(), canvas = new Target(), controls = new Target();
  const calls = [];
  let open = false;
  controls.getAttribute = name => name === 'aria-pressed' ? String(open) : null;
  doc.querySelector = selector => {
    if (selector === 'canvas') return canvas;
    if (selector === '[data-controls]') return controls;
    assert.fail(`unexpected host query: ${selector}`);
  };
  doc.hasFocus = () => true;
  doc.hidden = false;

  const observers = [], pending = new Set();
  canvas.attrs = {...tags('canvas')[0]};
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

  runInNewContext(script, {document: doc, MutationObserver});
  const exports = {
    ferravex_key: (...args) => calls.push(['key', ...args]),
    ferravex_blur: () => calls.push(['blur']),
    ferravex_focus: value => calls.push(['focus', value]),
    ferravex_controls_open: () => Number(open),
  };
  const bridge = bindInput(canvas, 'siege-foundry', () => loaded ? exports : undefined, () => true, {
    window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  const pageLink = new Target(), pageButton = new Target();
  pageButton.closest = () => pageButton;
  function key(type, code, target = canvas, extra = {}) {
    const event = {code, key: code, target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; }, ...extra};
    // Browser event propagation order; default focus navigation is left to the
    // browser, not simulated here. Fullscreen may handle Escape in capture.
    win.emit(type, event, true);
    if (!event.stopped) target.emit(type, event);
    if (!event.stopped) doc.emit(type, event);
    if (!event.stopped) win.emit(type, event);
    return event;
  }
  return {win, doc, canvas, pageLink, pageButton, key, calls, flush, panel(value) {
    open = value;
    assert.equal(bridge.syncPanel(), value);
  }};
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
  withPlayer(h, 'siege-foundry', (body, player) => {
    h.flush();
    const hostIds = ['range-objective', 'range-controls', 'range-remapping', 'range-focus'];
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

test('Tab leaves the range and page links retain Enter/Space/arrows without gameplay presses', () => {
  for (const loaded of [false, true]) {
    const h = fixture({loaded});
    for (const shiftKey of [false, true]) {
      const event = h.key('keydown', 'Tab', h.canvas, {shiftKey});
      assert.equal(event.defaultPrevented, false);
      assert.equal(event.stopped, true);
    }
    for (const target of [h.pageLink, h.pageButton]) {
      for (const code of ['Tab', 'Enter', 'Space', 'ArrowDown', 'Escape', 'F2', 'KeyR']) {
        assert.equal(h.key('keydown', code, target).defaultPrevented, false);
      }
    }
    assert.deepEqual(h.calls, [], 'native page interaction must not start, fire, rebuild or toggle a menu');
  }
});

test('focused gameplay and editor navigation retain the real shared keyboard ABI', () => {
  const h = fixture();
  // An alternate remappable key (J) crosses the same bridge as the defaults;
  // this source test does not claim a live Wasm profile-import round trip.
  for (const [code, value] of [['Enter', 0xff0d], ['Space', 32], ['ArrowLeft', 0xff51], ['KeyR', 114], ['KeyJ', 106], ['Escape', 0xff1b], ['F2', 0xffbf]]) {
    assert.equal(h.key('keydown', code).defaultPrevented, true);
    h.key('keyup', code);
    assert.deepEqual(h.calls.slice(-2), [['key', value, 1], ['key', value, 0]]);
  }
  h.panel(true);
  for (const [code, value] of [['Tab', 0xff09], ['ArrowUp', 0xff52], ['ArrowDown', 0xff54], ['Enter', 0xff0d], ['Escape', 0xff1b], ['F2', 0xffbf]]) {
    for (const shiftKey of [false, true]) {
      assert.equal(h.key('keydown', code, h.canvas, {shiftKey}).defaultPrevented, true);
      h.key('keyup', code);
      assert.deepEqual(h.calls.slice(-2), [['key', value, 1], ['key', value, 0]]);
    }
  }
  h.panel(false);
  const count = h.calls.length;
  assert.equal(h.key('keydown', 'Tab').defaultPrevented, false, 'editor closure restores page Tab');
  assert.equal(h.calls.length, count);
});

test('a held gameplay key releases after focus moves and browser shortcuts stay native', () => {
  const h = fixture();
  for (const target of [h.pageLink, h.pageButton]) {
    h.key('keydown', 'Space');
    h.key('keydown', 'Tab');
    h.key('keyup', 'Space', target);
    assert.deepEqual(h.calls.slice(-2), [['key', 32, 1], ['key', 32, 0]]);
  }
  const count = h.calls.length;
  for (const extra of [{ctrlKey: true}, {metaKey: true}, {altKey: true}, {isComposing: true}, {keyCode: 229}]) {
    assert.equal(h.key('keydown', 'KeyR', h.canvas, extra).defaultPrevented, false);
  }
  assert.equal(h.calls.length, count);
  h.win.emit('blur', {});
  assert.equal(h.key('keydown', 'Space').defaultPrevented, false);
  assert.deepEqual(h.calls.at(-1), ['blur']);
  h.win.emit('focus', {});
  assert.deepEqual(h.calls.at(-1), ['focus', 1]);
});

test('the local guard does not intercept earlier dialog or fullscreen Escape handling', () => {
  const h = fixture();
  h.pageButton.addEventListener('keydown', event => event.stopPropagation());
  assert.equal(h.key('keydown', 'Escape', h.pageButton).defaultPrevented, false);
  assert.deepEqual(h.calls, []);
  h.win.addEventListener('keydown', event => {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); }
  }, true);
  assert.equal(h.key('keydown', 'Escape').defaultPrevented, true);
  assert.deepEqual(h.calls, [], 'fullscreen capture owns Escape before game or page listeners');
});
