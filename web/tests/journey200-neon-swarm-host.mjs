// Source/DOM contract tests only: no generated dist, browser, or Wasm build.
import assert from 'node:assert/strict';
import {readFileSync, statSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const pageUrl = new URL('../neon-swarm/index.html', import.meta.url);
const html = readFileSync(pageUrl, 'utf8');
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const hostScript = html.match(/<script type="module">([\s\S]*?)<\/script>/)[1];
const attributes = source => Object.fromEntries([...source.matchAll(/([\w-]+)(?:="([^"]*)")?/g)].map(([, key, value]) => [key, value ?? '']));
const tag = id => {
  const match = html.match(new RegExp(`<([\\w-]+)\\b([^>]*\\bid="${id}"[^>]*)>`));
  assert.ok(match, `element #${id} exists`);
  return {name: match[1], attributes: attributes(match[2])};
};
const canvasAttributes = attributes(html.match(/<canvas\b([^>]*)>/)[1]);
const text = html.replace(/<script\b[^>]*>[\s\S]*?<\/script>/g, '').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ');

test('canvas describes the actual game and references visible, uniquely identified help', () => {
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>Neon Swarm · Ferravex<\/title>/);
  assert.equal((html.match(/<main>/g) || []).length, 1);
  assert.equal((html.match(/<canvas\b/g) || []).length, 1);
  assert.equal(canvasAttributes['aria-label'], 'Neon Swarm game canvas');
  assert.equal(canvasAttributes.tabindex, '0');
  assert.equal(canvasAttributes['aria-hidden'], undefined);
  assert.equal(canvasAttributes.role, undefined, 'do not force application or image semantics over an interactive canvas');
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length, 'IDREFs must resolve unambiguously');
  for (const id of canvasAttributes['aria-describedby'].split(' ')) {
    assert.equal(tag(id).attributes.hidden, undefined);
    assert.equal(tag(id).attributes['aria-hidden'], undefined);
  }
  assert.equal(tag('game-instructions').attributes.hidden, undefined);
  assert.equal(tag('game-instructions').attributes.tabindex, '-1', 'instruction link has a focusable destination');
  assert.equal(tag('game-instructions').attributes['aria-labelledby'], 'game-heading');
  assert.equal(tag('game-heading').name, 'h1');
  assert.match(text, /2D arena survival shooter\. Clear three waves of drones/);
  assert.match(text, /Amber rings and exclamation marks warn of melee attacks/);
  assert.match(html, /<canvas\b[^>]*>\s*Neon Swarm is a visual arena survival game\./);
  assert.match(html, /<noscript>[\s\S]*JavaScript is disabled/);
  const status = tag('game-status').attributes;
  assert.equal(status.role, 'status');
  assert.equal(status['aria-live'], 'polite');
  assert.equal(status['aria-atomic'], 'true');
});

test('guidance distinguishes defaults, saved bindings, game menus, and browser escape behavior', () => {
  for (const phrase of [
    'Default keyboard and mouse controls', 'Saved bindings may differ', 'Boost / dash',
    'W A S D', 'arrow keys', 'left mouse button', 'Space', 'Shift', 'Pause / menu',
    'R starts a fresh run', 'Play / confirm', 'Start', 'Resume', 'Play again',
    'Escape cancels a pending rebind', 'Closing Controls does not resume a paused run',
    'release held gameplay buttons', 'Escape may first leave fullscreen or close a player panel',
    'Shift + Tab', 'Details → Player tools',
  ]) assert.ok(text.includes(phrase), phrase);
  assert.doesNotMatch(text, /On touch|auto.?fire|tap the right|lower.left.*stick/i, 'no extra touch support claims');
  assert.doesNotMatch(html, /aria-keyshortcuts=/, 'remappable defaults must not be exposed as fixed shortcuts');
  assert.equal((html.match(/data-controls\b/g) || []).length, 1);
  assert.equal((html.match(/data-audio\b/g) || []).length, 1);
  assert.equal((html.match(/<button\b/g) || []).length, 2, 'shared runtime owns additional player controls');
  for (const kind of ['controls', 'audio']) {
    const button = attributes(html.match(new RegExp(`<button\\b([^>]*\\bdata-${kind}\\b[^>]*)>`))[1]);
    assert.equal(button.type, 'button');
    assert.equal(button.disabled, '');
    assert.equal(button['aria-pressed'], 'false');
  }
});

test('host keeps the catalog route, local boot module, and shared ABI/lifecycle entry point', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const game = catalog.find(game => game.id === canvasAttributes['data-game']);
  assert.equal(game.id, 'neon-swarm');
  assert.equal(game.host, 'neon-swarm');
  assert.equal(game.title, 'Neon Swarm');
  const scripts = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"[^>]*>/g)].map(match => match[1]);
  assert.deepEqual(scripts, ['../runtime.js']);
  assert.ok(statSync(new URL(scripts[0], pageUrl)).isFile());
  assert.ok(html.indexOf(hostScript) < html.indexOf('src="../runtime.js"'), 'accessibility handlers are installed before runtime boot');
  const runtime = readFileSync(new URL(scripts[0], pageUrl), 'utf8');
  assert.ok(runtime.includes("loadWasm(`${game.replaceAll('-','_')}.wasm`,imports)"));
  assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, new URL(`https://example.test/play/${game.host}/`)).pathname, '/play/neon-swarm/neon_swarm.wasm');
  for (const contract of ['bindInput(canvas,game', 'bindDetails(canvas,game', 'bindAudio(canvas', 'bindGraphicsLifecycle(canvas', 'suspendOnPageLifecycle:true', 'inputPlatform.suspend()', 'inputPlatform.resume()', 'exports.ferravex_init()', 'exports.ferravex_frame(']) {
    assert.ok(runtime.includes(contract), contract);
  }
  assert.doesNotMatch(hostScript, /ferravex_|WebAssembly|requestAnimationFrame|setInterval|dispatchEvent/, 'local helper does not duplicate ABI, frame scheduling or lifecycle events');
  for (const [, href] of html.matchAll(/\bhref="([^"]+)"/g)) {
    if (href.startsWith('#')) tag(href.slice(1));
    else {
      assert.equal(href, '../');
      assert.ok(statSync(new URL(`${href}index.html`, pageUrl)).isFile());
    }
  }
});

test('source styles keep focus, instructions and injected toolbar controls readable', () => {
  assert.match(css, /nav#game-toolbar\s*\{[^}]*flex-wrap:\s*wrap/);
  assert.match(css, /#game-toolbar > a\s*\{[^}]*display:\s*inline-flex/);
  assert.match(css, /#game-toolbar > button\s*\{[^}]*min-height:\s*44px[^}]*max-width:\s*100%[^}]*font:\s*inherit/);
  assert.match(css, /canvas:focus-visible\s*\{[^}]*outline:\s*3px solid/);
  assert.match(css, /a:focus-visible, button:focus-visible, \[tabindex\]:focus-visible/);
  assert.match(css, /@media\s*\(max-width:\s*42rem\)[\s\S]*grid-template-columns:\s*minmax\(0, 1fr\)/);
  assert.match(css, /#game-instructions\s*\{[^}]*overflow-wrap:\s*anywhere/);
  assert.doesNotMatch(css, /outline:\s*none|overflow:\s*hidden|\[role[=\s]/, 'do not hide page overflow or restyle every shared live status');
  assert.doesNotMatch(html, /user-scalable=no|maximum-scale=/);
});

// Minimal event propagation and observer doubles run the actual inline helper
// together with bindInput. They do not simulate native Tab order or CSS layout.
function fixture() {
  const observers = [];
  class Element {
    constructor(parent = null, attrs = {}) {
      this.parent = parent;
      this.attrs = {...attrs};
      this.listeners = new Map();
    }
    addEventListener(type, fn) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(fn);
    }
    getAttribute(name) { return this.attrs[name] ?? null; }
    setAttribute(name, value) {
      this.attrs[name] = String(value);
      for (const observer of observers) {
        if (observer.target === this && observer.options.attributeFilter.includes(name)) observer.pending = true;
      }
    }
    closest() { return this.isButton ? this : null; }
    focus() { doc.activeElement = this; }
    getBoundingClientRect() { return {left: 0, top: 0}; }
  }
  class MutationObserver {
    constructor(callback) { this.callback = callback; observers.push(this); }
    observe(target, options) { this.target = target; this.options = options; }
  }
  function flush() {
    let rounds = 0;
    while (observers.some(observer => observer.pending)) {
      assert.ok(++rounds <= 3, 'description repair must settle, not loop');
      for (const observer of observers) if (observer.pending) {
        observer.pending = false;
        observer.callback();
      }
    }
  }
  const win = new Element(), doc = new Element(win);
  const canvas = new Element(doc, canvasAttributes);
  const toolbar = new Element(doc), instructions = new Element(doc);
  const link = new Element(toolbar), helpLink = new Element(instructions), button = new Element(toolbar);
  button.isButton = true;
  doc.hidden = false;
  doc.hasFocus = () => true;
  doc.querySelector = selector => selector === 'canvas' ? canvas : null;
  doc.getElementById = id => ({'game-toolbar': toolbar, 'game-instructions': instructions})[id];
  runInNewContext(hostScript, {document: doc, MutationObserver});
  const calls = [];
  const exports = {};
  for (const name of ['key', 'blur', 'focus', 'input_settings']) exports[`ferravex_${name}`] = (...args) => calls.push([name, ...args]);
  const bridge = bindInput(canvas, 'neon-swarm', () => exports, () => true, {
    window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  function send(target, type, properties = {}) {
    const event = {
      target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; },
      ...properties,
    };
    for (let node = target; node; node = node.parent) {
      for (const listener of node.listeners.get(type) || []) listener(event);
      if (event.stopped) break;
    }
    return event;
  }
  return {canvas, toolbar, instructions, link, helpLink, button, win, doc, calls, bridge, send, flush};
}

test('shared player description replacement retains both game help and shared help without observer churn', () => {
  const f = fixture();
  f.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
  f.flush();
  assert.equal(f.canvas.getAttribute('aria-describedby'), 'game-description game-keyboard-help ferravex-canvas-help');
  f.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help future-help game-description');
  f.flush();
  assert.equal(f.canvas.getAttribute('aria-describedby'), 'game-description game-keyboard-help ferravex-canvas-help future-help');
  f.canvas.setAttribute('aria-describedby', '');
  f.flush();
  assert.equal(f.canvas.getAttribute('aria-describedby'), canvasAttributes['aria-describedby']);
});

test('native page keyboard navigation is not swallowed by the shared game input bridge', () => {
  const f = fixture();
  for (const target of [f.link, f.helpLink, f.button, f.instructions]) {
    for (const [key, code] of [['Tab', 'Tab'], ['Enter', 'Enter'], [' ', 'Space'], ['ArrowDown', 'ArrowDown'], ['r', 'KeyR']]) {
      const event = f.send(target, 'keydown', {key, code});
      assert.equal(event.defaultPrevented, false, `${key} keeps its browser default`);
    }
  }
  assert.deepEqual(f.calls, [], 'reading help or activating links must not send gameplay commands');
});

test('Shift+Tab offers native canvas exit while Tab, Escape, F2 and combat retain their shared route', () => {
  const f = fixture();
  const exit = f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true});
  assert.equal(exit.defaultPrevented, false, 'browser performs backward tab');
  assert.equal(exit.stopped, true, 'shared bridge cannot consume the exit');
  assert.deepEqual(f.calls, []);
  for (const [key, code, rust] of [['Tab', 'Tab', 0xff09], ['Escape', 'Escape', 0xff1b], ['F2', 'F2', 0xffbf], ['Enter', 'Enter', 0xff0d], [' ', 'Space', 32], ['w', 'KeyW', 119]]) {
    assert.equal(f.send(f.canvas, 'keydown', {key, code}).defaultPrevented, true);
    f.send(f.canvas, 'keyup', {key, code});
    assert.deepEqual(f.calls.slice(-2), [['key', rust, 1], ['key', rust, 0]]);
  }
  const count = f.calls.length;
  f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true, ctrlKey: true});
  f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true, isComposing: true});
  assert.equal(f.calls.length, count, 'browser shortcuts and composition keep shared suppression');
});

test('held keys still release after leaving the canvas; page lifecycle suspension remains available', () => {
  const f = fixture();
  f.send(f.canvas, 'keydown', {key: 'w', code: 'KeyW'});
  f.send(f.button, 'keyup', {key: 'w', code: 'KeyW'});
  assert.deepEqual(f.calls.slice(-2), [['key', 119, 1], ['key', 119, 0]]);
  f.send(f.win, 'pagehide');
  assert.deepEqual(f.calls.at(-1), ['blur']);
  const count = f.calls.length;
  f.send(f.canvas, 'keydown', {key: 'w', code: 'KeyW'});
  assert.equal(f.calls.length, count);
  f.send(f.win, 'pageshow');
  assert.deepEqual(f.calls.at(-1), ['focus', 1]);
  f.bridge.openSettings();
  assert.deepEqual(f.calls.at(-1), ['input_settings'], 'Controls keeps its existing Wasm request');
});
