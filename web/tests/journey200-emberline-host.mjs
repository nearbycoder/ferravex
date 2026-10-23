// Source/DOM contract checks only: no generated dist, Wasm build or live playtest.
import assert from 'node:assert/strict';
import {readFileSync, statSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const pageUrl = new URL('../emberline/index.html', import.meta.url);
const html = readFileSync(pageUrl, 'utf8');
const script = html.match(/<script id="emberline-host-accessibility">([\s\S]*?)<\/script>/)[1];
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const attributes = text => Object.fromEntries([...text.matchAll(/([\w-]+)(?:="([^"]*)")?/g)].map(([, key, value]) => [key, value ?? '']));
const elements = tag => [...html.matchAll(new RegExp(`<${tag}\\b([^>]*)>([\\s\\S]*?)<\\/${tag}>`, 'g'))]
  .map(([, attrs, content]) => ({attrs: attributes(attrs), content}));
const canvasMarkup = elements('canvas')[0];
const text = content => content.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
const ids = new Map([...html.matchAll(/<([a-z][\w-]*)\b([^>]*\bid="[^"]+"[^>]*)>/g)]
  .map(([, tag, attrs]) => { const parsed = attributes(attrs); return [parsed.id, {tag, attrs: parsed}]; }));

function assertVisibleReference(id) {
  assert.ok(ids.has(id), `missing accessible reference: ${id}`);
  assert.equal(ids.get(id).attrs.hidden, undefined);
  assert.notEqual(ids.get(id).attrs['aria-hidden'], 'true');
}

test('canvas has a stable name, visible game description and a usable fallback', () => {
  assert.match(html, /<html lang="en">/);
  assert.equal(text(elements('title')[0].content), 'Emberline · Ferravex');
  assert.equal(elements('h1').length, 1);
  assert.equal(elements('canvas').length, 1);
  assert.equal(canvasMarkup.attrs.tabindex, '0');
  assert.equal(canvasMarkup.attrs['data-game'], 'emberline');
  assert.equal(canvasMarkup.attrs['aria-labelledby'], 'emberline-title');
  assert.notEqual(canvasMarkup.attrs.role, 'img', 'the interactive canvas must not be presented as a static image');
  assert.notEqual(canvasMarkup.attrs.role, 'application', 'do not force application mode for the whole game');
  for (const attr of ['aria-labelledby', 'aria-describedby']) {
    for (const id of canvasMarkup.attrs[attr].split(/\s+/)) assertVisibleReference(id);
  }
  assert.doesNotMatch(html, /\b(?:hidden|aria-hidden)\s*(?:=|>)/, 'host instructions must not be hidden');
  assert.match(text(canvasMarkup.content), /visual 2D platformer.*JavaScript, WebAssembly and WebGL2/);
  assert.match(html, /<noscript>.*Enable JavaScript/);
  assert.equal(ids.size, [...html.matchAll(/\bid="[^"]+"/g)].length, 'IDs must be unique');
  assert.match(text(html), /green exit lift.*numbered lantern.*checkpoint.*Embers are optional/);
  const statuses = elements('p').filter(p => p.attrs.role === 'status');
  assert.equal(statuses.length, 1, 'runtime must find the loading status before injected player statuses');
  assert.equal(statuses[0].attrs.id, 'emberline-status');
  assert.equal(statuses[0].attrs['aria-live'], 'polite');
  assert.equal(statuses[0].attrs['aria-atomic'], 'true');
  assert.equal(text(statuses[0].content), 'Loading Emberline…');
  assert.doesNotMatch(css, /\[role\s*=\s*["']?status/, 'host styling must not reposition shared player status messages');
});

test('guide labels defaults honestly and explains focus, editor escape and player tools', () => {
  const guide = text(html.slice(html.indexOf('<section id="emberline-guide"'), html.indexOf('<script')));
  assert.match(guide, /default bindings.*current bindings.*Saved bindings take precedence/);
  const definitions = elements('dl')[0].content;
  for (const control of [
    /Enter.*starts, resumes a paused run, or retries/,
    /A.*D.*Left.*Right.*arrow keys/,
    /Space.*W.*Up.*Tap for a short hop; hold for a higher jump/,
    /Escape.*during a run/,
    /R.*resets the whole run.*even while paused/,
  ]) assert.match(text(definitions), control);
  assert.match(guide, /Pause before moving to page controls/);
  assert.match(guide, /With Controls closed, Tab and Shift \+ Tab/);
  assert.match(guide, /reserves Tab for navigation; choose another key when remapping gameplay/);
  assert.match(guide, /Escape cancels a binding capture; press it again to close Controls/);
  assert.match(guide, /Closing Controls does not resume a manually paused run/);
  assert.match(guide, /Details → Player tools/);
  assert.match(guide, /Done or Escape to close Details and return focus/);
  assert.match(guide, /Fullscreen may use Escape to exit first/);
  assert.match(guide, /default layout.*current touch layout/);
  // These source checks make future task 131 control changes a review point.
  const game = readFileSync(new URL('../../examples/emberline/src/lib.rs', import.meta.url), 'utf8');
  assert.match(game, /PlayerInput::preset\("emberline", Preset::Platformer\)/);
  assert.match(game, /set_touch_layout/);
  const preset = readFileSync(new URL('../../crates/ferravex-render/src/input/presets.rs', import.meta.url), 'utf8');
  assert.match(preset, /"move_left", "Left", KeyCode::A, KeyCode::Left/);
  assert.match(preset, /"move_right", "Right", KeyCode::D, KeyCode::Right/);
  assert.match(preset, /bindings\.extend\(\[key\(KeyCode::W\), key\(KeyCode::Up\)\]\)/);
});

test('relative catalog/runtime routes and original shared-control hooks stay intact', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const entry = catalog.find(game => game.id === canvasMarkup.attrs['data-game']);
  assert.equal(entry.host, 'emberline');
  assert.equal(entry.title, 'Emberline');
  const boot = elements('script').filter(s => s.attrs.src);
  assert.equal(boot.length, 1);
  assert.equal(boot[0].attrs.type, 'module');
  assert.equal(boot[0].attrs.src, '../runtime.js');
  assert.ok(statSync(new URL(boot[0].attrs.src, pageUrl)).isFile());
  const links = elements('a');
  const back = links.find(link => link.attrs.href === '../');
  assert.ok(back);
  assert.ok(statSync(new URL('../index.html', pageUrl)).isFile());
  for (const link of links.filter(link => link.attrs.href.startsWith('#'))) {
    assertVisibleReference(link.attrs.href.slice(1));
    assert.ok(['0', '-1'].includes(ids.get(link.attrs.href.slice(1)).attrs.tabindex), 'in-page navigation has a focus destination');
  }
  for (const base of ['https://example.test/emberline/', 'https://example.test/play/emberline/', 'https://example.test/play/emberline/index.html?look=ink&autoplay']) {
    const root = new URL('../', base);
    assert.equal(new URL(back.attrs.href, base).href, root.href);
    assert.equal(new URL(boot[0].attrs.src, base).href, new URL('runtime.js', root).href);
    assert.equal(new URL('emberline.wasm', base).pathname, `${root.pathname}emberline/emberline.wasm`);
  }
  const buttons = elements('button');
  assert.equal(buttons.length, 2, 'Details, Fullscreen and player tools remain shared runtime controls');
  for (const hook of ['data-controls', 'data-audio']) {
    const matches = buttons.filter(button => Object.hasOwn(button.attrs, hook));
    assert.equal(matches.length, 1);
    assert.equal(matches[0].attrs.type, 'button');
    assert.equal(matches[0].attrs['aria-pressed'], 'false');
    assert.ok(Object.hasOwn(matches[0].attrs, 'disabled'), 'shared runtime enables controls after loading');
  }
  assert.doesNotMatch(script, /WebAssembly|fetch\(|requestAnimationFrame|ferravex_/, 'local accessibility adapter does not replace boot, ABI or timing');
});

test('layout provides focus, wrapping controls, readable narrow instructions and fullscreen scrolling', () => {
  assert.match(css, /#emberline-page > nav\s*\{[^}]*flex-wrap:\s*wrap[^}]*height:\s*auto/s);
  assert.match(css, /#emberline-page > nav > a\s*\{[^}]*display:\s*block/s, 'catalog link remains visible despite shared small-screen styles');
  assert.match(css, /#emberline-page > nav button\s*\{[^}]*min-height:\s*44px/s);
  assert.match(css, /#emberline-page :focus-visible\s*\{[^}]*outline:\s*3px solid/s);
  assert.doesNotMatch(css, /outline:\s*(?:none|0)\b/);
  assert.match(css, /@media \(max-width: 40rem\)[\s\S]*grid-template-columns:\s*minmax\(0, 1fr\)/);
  assert.match(css, /#emberline-guide\s*\{[^}]*user-select:\s*text[^}]*touch-action:\s*auto/s);
  assert.match(css, /#emberline-page\.ferravex-fullscreen main\s*\{[^}]*overflow:\s*auto/s);
  assert.match(css, /forced-colors: active/);
});

// Minimal event/attribute fixture. Run the actual inline adapter against the
// real shared input bridge; native browser default focus movement is not mocked.
class Target {
  constructor(attrs = {}) { this.attrs = {...attrs}; this.listeners = new Map(); this.focusCount = 0; }
  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(fn);
  }
  dispatch(type, event) { for (const listener of this.listeners.get(type) ?? []) listener(event); }
  getAttribute(name) { return this.attrs[name] ?? null; }
  setAttribute(name, value) { this.attrs[name] = String(value); this.changed?.(name); }
  removeAttribute(name) { delete this.attrs[name]; this.changed?.(name); }
  closest() { return null; }
  focus() { this.focusCount++; }
}
function fixture() {
  const canvas = new Target(canvasMarkup.attrs);
  const controls = new Target({'aria-pressed': 'false'});
  const document = new Target(), window = new Target(), calls = [], queued = new Set();
  document.hasFocus = () => true;
  document.querySelector = selector => {
    if (selector === 'canvas[data-game="emberline"]') return canvas;
    if (selector === '[data-controls]') return controls;
    throw Error(`Unexpected query: ${selector}`);
  };
  let observers = 0;
  class MutationObserver {
    constructor(callback) { this.callback = callback; observers++; }
    observe(target, options) {
      assert.equal(target, canvas);
      assert.equal(options.attributes, true);
      assert.deepEqual(Array.from(options.attributeFilter), ['aria-describedby']);
      target.changed = name => { if (options.attributeFilter.includes(name)) queued.add(this.callback); };
    }
  }
  runInNewContext(script, {document, MutationObserver});
  const exports = {ferravex_key: (...args) => calls.push(args)};
  const bridge = bindInput(canvas, 'emberline', () => exports, () => true, {
    window, document, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  return {canvas, controls, document, window, calls, bridge, get observers() { return observers; },
    flush() {
      let turns = 0;
      while (queued.size) {
        assert.ok(++turns < 4, 'description updates must settle rather than loop');
        const pending = [...queued]; queued.clear(); for (const callback of pending) callback();
      }
    },
    key(target, code, type = 'keydown', extra = {}) {
      const event = {target, code, key: code.startsWith('Key') ? code.slice(3).toLowerCase() : code,
        defaultPrevented: false, stopped: false, ...extra,
        preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; }};
      document.dispatch(type, event);
      if (!event.stopped) window.dispatch(type, event);
      return event;
    },
  };
}

test('shared help replacement retains the host description without accumulation or focus changes', () => {
  const h = fixture();
  assert.equal(h.observers, 1);
  const expected = canvasMarkup.attrs['aria-describedby'].split(/\s+/);
  h.canvas.setAttribute('aria-label', 'Emberline game canvas'); // bindPlayer's shared fallback name.
  h.canvas.setAttribute('aria-describedby', 'ferravex-canvas-help');
  h.flush();
  assert.deepEqual(h.canvas.getAttribute('aria-describedby').split(' '), ['ferravex-canvas-help', ...expected]);
  assert.equal(h.canvas.getAttribute('aria-labelledby'), 'emberline-title', 'host name takes precedence over the fallback label');
  for (let i = 0; i < 3; i++) {
    h.canvas.setAttribute('aria-describedby', h.canvas.getAttribute('aria-describedby'));
    h.flush();
    assert.equal(h.canvas.getAttribute('aria-describedby').split(' ').length, expected.length + 1);
  }
  h.canvas.setAttribute('aria-describedby', 'replacement-help'); h.flush();
  assert.deepEqual(h.canvas.getAttribute('aria-describedby').split(' '), ['replacement-help', ...expected]);
  h.canvas.removeAttribute('aria-describedby'); h.flush();
  assert.equal(h.canvas.getAttribute('aria-describedby'), expected.join(' '));
  assert.equal(h.canvas.focusCount, 0);
});

test('Tab leaves the canvas normally while Controls retains its existing Tab navigation', () => {
  const h = fixture();
  for (const shiftKey of [false, true]) {
    assert.equal(h.key(h.canvas, 'Tab', 'keydown', {shiftKey}).defaultPrevented, false);
    assert.equal(h.calls.length, 0);
  }
  h.controls.setAttribute('aria-pressed', 'true');
  assert.equal(h.key(h.canvas, 'Tab').defaultPrevented, true);
  assert.deepEqual(h.calls, [[0xff09, 1]]);
  h.key(h.canvas, 'Tab', 'keyup');
  assert.deepEqual(h.calls.at(-1), [0xff09, 0]);
  h.controls.setAttribute('aria-pressed', 'false');
  assert.equal(h.key(h.canvas, 'Tab').defaultPrevented, false);
});

test('page links and guide keys retain native behavior without firing game actions', () => {
  const h = fixture(), link = new Target(), guide = new Target();
  for (const target of [link, guide, h.controls]) {
    for (const key of ['Enter', 'Space', 'Escape', 'Tab', 'ArrowDown', 'KeyR', 'F2']) {
      assert.equal(h.key(target, key).defaultPrevented, false);
    }
  }
  assert.deepEqual(h.calls, []);
  // A release after moving focus still clears a key held on the canvas.
  h.key(h.canvas, 'KeyD');
  h.key(link, 'KeyD', 'keyup');
  assert.deepEqual(h.calls, [[100, 1], [100, 0]]);
});

test('game/menu keys still reach the real bridge and keep repeat, IME and modifier guards', () => {
  const h = fixture();
  for (const [code, value] of [['Enter', 0xff0d], ['Escape', 0xff1b], ['F2', 0xffbf], ['Space', 32], ['KeyW', 119], ['ArrowUp', 0xff52], ['KeyR', 114]]) {
    h.key(h.canvas, code);
    assert.deepEqual(h.calls.at(-1), [value, 1]);
    h.key(h.canvas, code, 'keyup');
    assert.deepEqual(h.calls.at(-1), [value, 0]);
  }
  const before = h.calls.length;
  for (const extra of [{repeat: true}, {isComposing: true}, {keyCode: 229}, {ctrlKey: true}, {altKey: true}, {metaKey: true}]) {
    h.key(h.canvas, 'KeyR', 'keydown', extra);
  }
  assert.equal(h.calls.length, before);
  h.controls.setAttribute('aria-pressed', 'true');
  h.key(h.canvas, 'Escape');
  assert.deepEqual(h.calls.at(-1), [0xff1b, 1], 'renderer still owns capture cancel/editor close');
  assert.equal(h.canvas.focusCount, 0, 'local adapter does not steal focus');
});
