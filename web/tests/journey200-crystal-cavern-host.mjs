// Source/structure and event-contract checks only: no dist artifacts, Wasm or
// browser layout. Run: node web/tests/journey200-crystal-cavern-host.mjs
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const hostUrl = new URL('../crystal-cavern/index.html', import.meta.url);
const html = readFileSync(hostUrl, 'utf8');
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const runtime = read('../runtime.js');
const gameplay = read('../../examples/crystal-cavern/src/lib.rs');
const presets = read('../../crates/ferravex-render/src/input/presets.rs');
const player = read('../player.js');
const markup = html.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, '');
const text = value => value.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const hostScript = html.match(/<script id="expedition-accessibility">([\s\S]*?)<\/script>/)[1];

// These helpers inspect this controlled source, not arbitrary HTML or a browser
// accessibility tree. Event behavior below executes the actual inline script.
function attributes(opening) {
  return Object.fromEntries([...opening.matchAll(/([\w:-]+)(?:="([^"]*)")?/g)]
    .slice(1).map(([, name, value]) => [name, value ?? true]));
}
const tags = tag => [...markup.matchAll(new RegExp(`<${tag}\\b[^>]*>`, 'g'))].map(match => attributes(match[0]));
function element(id) {
  const match = markup.match(new RegExp(`<([\\w-]+)\\b([^>]*\\bid="${id}"[^>]*)>([\\s\\S]*?)<\\/\\1>`));
  assert.ok(match, `element #${id} exists`);
  return {attributes: attributes(`<${match[1]}${match[2]}>`), text: text(match[3])};
}

test('canvas has a real game name, visible objective/keyboard/remapping descriptions and fallback', () => {
  assert.equal(tags('html')[0].lang, 'en');
  assert.equal(text(markup.match(/<title>(.*?)<\/title>/)[1]), 'Crystal Cavern · Ferravex');
  assert.equal(element('expedition-title').text, 'Crystal Cavern');
  assert.equal(tags('main').length, 1);
  const [canvas] = tags('canvas');
  assert.equal(tags('canvas').length, 1);
  assert.equal(canvas.tabindex, '0');
  assert.equal(canvas['aria-label'], 'Crystal Cavern game canvas');
  assert.equal(canvas['aria-hidden'], undefined);
  assert.equal(canvas.role, undefined, 'do not replace canvas semantics with an application or image');
  const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length, 'description IDs must be unambiguous');
  const descriptions = canvas['aria-describedby'].split(/\s+/);
  assert.deepEqual(descriptions, ['expedition-controls', 'expedition-keyboard', 'expedition-remapping']);
  for (const id of descriptions) {
    const node = element(id);
    assert.ok(node.text.length > 60);
    assert.equal(node.attributes.hidden, undefined);
    assert.equal(node.attributes['aria-hidden'], undefined);
  }
  assert.match(element('expedition-controls').text, /three crystals, awaken altar I then altar II/);
  assert.match(element('expedition-keyboard').text, /Tab.*Shift.*Tab.*page controls/);
  assert.match(element('expedition-remapping').text, /Saved or remapped bindings may differ/);
  assert.match(element('expedition-canvas').text, /JavaScript, WebAssembly and WebGL2/);
  assert.equal(tags('noscript').length, 1);
  assert.equal(tags('section')[0]['aria-labelledby'], 'expedition-guide-title');
  assert.equal(tags('section')[0].tabindex, '0', 'Tab from canvas reaches the readable guide');
  assert.ok(markup.indexOf('<canvas') < markup.indexOf('<section'));
  assert.match(tags('meta').find(meta => meta.name === 'viewport').content, /width=device-width/);
  assert.doesNotMatch(html, /user-scalable=no|maximum-scale=1/);
});

test('documented defaults follow the current Explore preset and game overrides', () => {
  const guide = element('expedition-guide').text;
  assert.match(guide, /Default keyboard and mouse controls/);
  assert.match(guide, /W \/ A \/ S \/ D/);
  assert.match(guide, /Arrow keys or left-button mouse drag/);
  assert.match(guide, /E — recover a crystal/);
  assert.match(guide, /Enter or the expedition's on-screen button/);
  assert.match(guide, /R — clears current expedition progress/);
  assert.match(guide, /Toggle the game's hint H/);
  assert.match(gameplay, /PlayerInput::preset\("crystal-cavern", Preset::Explore\)/);
  assert.match(gameplay, /set_mouse_look_button\(Some\(1\)\)/);
  for (const [action, key] of [['move_left', 'A'], ['move_right', 'D'], ['move_forward', 'W'], ['move_back', 'S']]) {
    assert.ok(gameplay.includes(`("${action}", KeyCode::${key})`), `${action} default`);
  }
  for (const key of ['Left', 'Right', 'Up', 'Down', 'H']) assert.ok(gameplay.includes(`KeyCode::${key}`));
  for (const [action, key] of [['interact', 'E'], ['restart', 'R'], ['confirm', 'Enter']]) {
    assert.match(presets, new RegExp(`"${action}"[\\s\\S]*?key\\(KeyCode::${key}\\)`));
  }
  assert.match(presets, /let pause_key =[\s\S]*?KeyCode::Escape/);
  assert.match(guide, /Escape cancels an active remap; otherwise it closes Controls/);
  assert.match(guide, /Closing Controls preserves any manual pause/);
  assert.match(guide, /Details → Player tools/);
  assert.doesNotMatch(guide, /touch|tap|swipe/i, 'do not promise device support in this source-only pass');
});

test('shared boot, renderer hooks, catalog and relative routes remain intact', () => {
  const catalog = JSON.parse(read('../../examples/catalog.json'));
  const game = catalog.find(entry => entry.id === 'crystal-cavern');
  assert.equal(game.host, 'crystal-cavern');
  assert.equal(game.title, element('expedition-title').text);
  assert.equal(tags('canvas')[0]['data-game'], game.id);
  const scripts = [...html.matchAll(/<script\b[^>]*>/g)].map(match => attributes(match[0]));
  assert.deepEqual(scripts.filter(script => script.src), [{type: 'module', src: '../runtime.js'}]);
  assert.equal(scripts.length, 2, 'one local accessibility guard and the existing boot entry');
  assert.ok(existsSync(new URL('../runtime.js', hostUrl)));
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-','_'\)\}\.wasm`,imports\)/);
  assert.match(runtime, /exports\.ferravex_init\(\)/);
  for (const module of ['loader.js', 'retained.js', 'effects.js', 'details.js', 'input.js', 'audio.js', 'lifecycle.js']) {
    assert.ok(runtime.includes(`from './${module}'`), `${module} still supplied by runtime`);
    assert.ok(existsSync(new URL(`../${module}`, import.meta.url)));
  }
  assert.match(runtime, /new URLSearchParams\(location.search\)\.has\('material-tools'\)/);
  assert.match(runtime, /import\('\.\/material-tools\.js'\)/);
  assert.match(runtime, /inputPlatform\.openSettings\(\);canvas\.focus\(\)/);
  assert.match(runtime, /canvas\.getBoundingClientRect\(\)/, 'nested canvas still supplies logical dimensions');
  for (const prefix of ['/web/', '/play/']) {
    const route = new URL(`${prefix}${game.host}/?material-tools&look=warm`, 'https://example.test');
    assert.equal(new URL('../runtime.js', route).pathname, `${prefix}runtime.js`);
    assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, route).pathname, `${prefix}crystal-cavern/crystal_cavern.wasm`);
    for (const link of tags('a')) assert.equal(new URL(link.href, route).pathname, prefix);
  }
  const controls = tags('button').filter(button => button['data-controls'] !== undefined);
  const audio = tags('button').filter(button => button['data-audio'] !== undefined);
  assert.equal(controls.length, 1);
  assert.equal(audio.length, 1);
  for (const button of [...controls, ...audio]) {
    assert.equal(button.type, 'button');
    assert.equal(button.disabled, true, 'shared runtime owns readiness');
    assert.equal(button['aria-pressed'], 'false');
  }
  assert.equal(tags('button').length, 2, 'Details, fullscreen and player tools are created only by shared modules');
  const status = element('expedition-status').attributes;
  assert.equal(status.role, 'status');
  assert.equal(status['aria-live'], 'polite');
  assert.equal(status['aria-atomic'], 'true');
  assert.doesNotMatch(hostScript, /fetch\(|WebAssembly|requestAnimationFrame|setInterval|ferravex_/, 'local guard does not own boot, frames or Wasm ABI');
});

test('source styles support focus, wrapping and readable guides without shared selector collisions', () => {
  assert.match(css, /\.cavern-host :focus-visible\s*\{[^}]*outline:\s*3px solid/);
  assert.match(css, /#expedition-canvas:focus-visible\s*\{[^}]*outline-offset:\s*-4px/);
  assert.match(css, /#cavern-navigation\s*\{[^}]*flex-wrap:\s*wrap[^}]*height:\s*auto/);
  assert.match(css, /#cavern-navigation button\s*\{[^}]*min-height:\s*44px/);
  assert.match(css, /minmax\(min\(100%, 16rem\), 1fr\)/);
  assert.match(css, /#expedition-guide\s*\{[^}]*overflow-wrap:\s*anywhere/);
  assert.match(css, /body\.cavern-host\.ferravex-fullscreen\s*\{\s*overflow:\s*auto/);
  assert.doesNotMatch(css, /\[role[=\s]|outline:\s*none|overflow:\s*hidden/);
  assert.match(markup, /<header>\s*<nav/, 'shared mobile body > nav rules must not hide the catalog link');
  assert.match(markup, /<main>\s*<div[^>]*>\s*<canvas/, 'shared body > canvas height rules cannot squash this stage');
});

function fixture() {
  class Target {
    listeners = new Map();
    addEventListener(type, listener, options) {
      const entries = this.listeners.get(type) ?? [];
      entries.push({listener, capture: options === true || !!options?.capture});
      this.listeners.set(type, entries);
    }
    emit(type, event, capture = false) {
      for (const entry of this.listeners.get(type) ?? []) {
        if (entry.capture === capture) entry.listener(event);
        if (event.immediate) break;
      }
    }
  }
  const win = new Target(), doc = new Target(), canvas = new Target();
  const attrs = new Map(Object.entries(tags('canvas')[0]));
  let observed, mutations = [], writes = 0;
  canvas.getAttribute = name => attrs.get(name) ?? null;
  canvas.setAttribute = (name, value) => {
    attrs.set(name, value); writes++;
    if (observed?.options.attributeFilter.includes(name)) mutations.push({attributeName: name});
  };
  canvas.closest = () => null;
  canvas.focus = () => { doc.activeElement = canvas; };
  doc.activeElement = canvas;
  doc.hasFocus = () => true;
  doc.hidden = false;
  doc.getElementById = id => id === 'expedition-canvas' ? canvas : null;
  class MutationObserver {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { assert.equal(target, canvas); observed = {callback: this.callback, options}; }
  }
  runInNewContext(hostScript, {document: doc, MutationObserver});
  const calls = [];
  const exports = {ferravex_key: (...args) => calls.push(['key', ...args]), ferravex_blur: () => calls.push(['blur']), ferravex_focus: value => calls.push(['focus', value])};
  bindInput(canvas, 'crystal-cavern', () => exports, () => true, {
    window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  function key(type, code, target = canvas, extra = {}) {
    const event = {code, key: code, target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; },
      stopImmediatePropagation() { this.stopped = this.immediate = true; }, ...extra};
    // Model the relevant DOM propagation order, including fullscreen capture.
    win.emit(type, event, true);
    if (!event.stopped) target.emit?.(type, event);
    if (!event.stopped) doc.emit(type, event);
    if (!event.stopped) win.emit(type, event);
    return event;
  }
  function flush() {
    let rounds = 0;
    while (mutations.length) {
      assert.ok(++rounds <= 3, 'description observer must settle, not loop');
      const pending = mutations; mutations = [];
      observed.callback(pending);
    }
  }
  return {canvas, doc, win, calls, key, flush, observed, writes: () => writes};
}

test('game-specific description survives shared player initialization without mutation loops', () => {
  assert.match(player, /canvas\.setAttribute\('aria-describedby',help\.id\)/, 'exercise the current shared overwrite contract');
  const f = fixture();
  assert.equal(f.observed.options.attributes, true);
  assert.deepEqual(Array.from(f.observed.options.attributeFilter), ['aria-describedby']);
  for (const shared of ['ferravex-canvas-help', 'ferravex-canvas-help expedition-controls', '', 'other-help other-help']) {
    f.canvas.setAttribute('aria-describedby', shared);
    f.flush();
    const ids = f.canvas.getAttribute('aria-describedby').split(/\s+/);
    for (const id of ['expedition-controls', 'expedition-keyboard', 'expedition-remapping', ...shared.split(/\s+/).filter(Boolean)]) assert.ok(ids.includes(id));
    assert.equal(new Set(ids).size, ids.length);
    const writes = f.writes();
    f.flush();
    assert.equal(f.writes(), writes);
  }
});

test('Tab escapes canvas and page links/guide retain native activation and scrolling', () => {
  const f = fixture();
  const link = {closest: () => null}, button = {closest: () => ({})};
  for (const target of [f.canvas, link, button]) {
    for (const shiftKey of [false, true]) for (const type of ['keydown', 'keyup']) {
      const event = f.key(type, 'Tab', target, {shiftKey});
      assert.equal(event.defaultPrevented, false, 'browser retains Tab traversal');
    }
  }
  for (const code of ['Enter', 'Space', 'ArrowDown', 'KeyR', 'KeyE']) {
    const event = f.key('keydown', code, link);
    assert.equal(event.defaultPrevented, false, `page ${code} retains its native action`);
  }
  f.key('keydown', 'Enter', button);
  assert.deepEqual(f.calls, [], 'DOM navigation must not start, restart or move the game');
});

test('canvas menu/remap keys still reach the real bridge once, including release after focus moves', () => {
  const f = fixture();
  for (const [code, numeric] of [['Enter', 0xff0d], ['Escape', 0xff1b], ['F2', 0xffbf], ['KeyE', 101], ['ArrowDown', 0xff54], ['KeyR', 114]]) {
    const down = f.key('keydown', code);
    assert.equal(down.defaultPrevented, true);
    f.key('keyup', code);
    assert.deepEqual(f.calls.slice(-2), [['key', numeric, 1], ['key', numeric, 0]]);
  }
  f.key('keydown', 'KeyW');
  const pageButton = {closest: () => ({})};
  f.doc.activeElement = pageButton;
  f.key('keyup', 'KeyW', pageButton);
  assert.deepEqual(f.calls.slice(-2), [['key', 119, 1], ['key', 119, 0]], 'a held movement key cannot become stuck on Tab out');
  const before = f.calls.length;
  f.key('keydown', 'KeyR', f.canvas, {ctrlKey: true});
  assert.equal(f.calls.length, before, 'browser shortcuts remain available');
});

test('fullscreen capture and window/visibility lifecycle keep their existing priority', () => {
  const f = fixture();
  let escaped = 0;
  f.win.addEventListener('keydown', event => {
    if (event.key === 'Escape') { escaped++; event.preventDefault(); event.stopImmediatePropagation(); }
  }, true);
  f.key('keydown', 'Escape');
  assert.equal(escaped, 1);
  assert.deepEqual(f.calls, [], 'fullscreen capture precedes document and game handlers');
  f.win.emit('blur', {});
  f.win.emit('focus', {});
  f.doc.hidden = true;
  f.doc.emit('visibilitychange', {});
  f.doc.hidden = false;
  f.doc.emit('visibilitychange', {});
  assert.deepEqual(f.calls, [['blur'], ['focus', 1], ['blur'], ['focus', 1]]);
});
