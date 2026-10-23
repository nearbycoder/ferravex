// Source contracts and real host-script/input-bridge behavior; no staged Wasm.
// Event doubles below do not establish browser layout, native Tab traversal,
// screen-reader support, or gameplay correctness.
import assert from 'node:assert/strict';
import {readFileSync, statSync} from 'node:fs';
import test from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const page = new URL('../salvage-pinball/index.html', import.meta.url);
const html = readFileSync(page, 'utf8');
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
function attributes(source) {
  return Object.fromEntries([...source.matchAll(/([\w-]+)(?:="([^"]*)")?/g)].map(([, name, value]) => [name, value ?? '']));
}
const elements = [...html.matchAll(/<(\w+)\b([^>]*)>/g)].map(([, tag, attrs]) => ({tag, attrs: attributes(attrs)}));
const withId = id => elements.find(element => element.attrs.id === id);
const text = source => source.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
const paragraph = id => text(html.match(new RegExp(`<p id="${id}">([\\s\\S]*?)<\\/p>`))[1]);
const inline = scripts.filter(([, attrs]) => !attributes(attrs).src);

test('named canvas points to visible purpose, default bindings and keyboard exit instructions', () => {
  const ids = elements.filter(element => element.attrs.id).map(element => element.attrs.id);
  assert.equal(ids.length, new Set(ids).size, 'IDs must be unique');
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>Salvage Pinball · Ferravex<\/title>/);
  assert.match(html, /<h1>Salvage Pinball<\/h1>/);
  assert.equal(elements.filter(element => element.tag === 'main').length, 1);
  assert.equal(elements.filter(element => element.tag === 'canvas').length, 1);
  const canvas = withId('pinball-table');
  assert.equal(canvas.attrs.tabindex, '0');
  assert.equal(canvas.attrs['aria-label'], 'Salvage Pinball table');
  assert.equal(canvas.attrs.role, undefined, 'do not force screen-reader application mode');
  const references = canvas.attrs['aria-describedby'].split(/\s+/);
  assert.deepEqual(references, ['pinball-description', 'pinball-keys', 'pinball-navigation']);
  for (const id of references) {
    assert.equal(withId(id).tag, 'p');
    assert.ok(!('hidden' in withId(id).attrs));
    assert.ok(!('aria-hidden' in withId(id).attrs));
    assert.ok(html.indexOf(`id="${id}"`) < html.indexOf('<details'), 'description is outside collapsed help');
    assert.ok(paragraph(id).length > 30);
  }
  assert.match(paragraph('pinball-description'), /three balls.*bumpers/i);
  assert.match(paragraph('pinball-keys'), /Default keys:.*A.*Left arrow.*D.*Right arrow.*Space.*Escape/);
  assert.match(paragraph('pinball-keys'), /Controls.*F2.*current bindings/);
  assert.match(paragraph('pinball-navigation'), /Shift \+ Tab.*How to play.*does not pause/);
  assert.match(html, /Saved remaps take precedence/);
  assert.match(html, /<canvas\b[^>]*>[^<]*JavaScript, WebAssembly and WebGL2/);
  assert.match(html, /<noscript>[\s\S]*Enable JavaScript/);
  const status = withId('pinball-status').attrs;
  assert.equal(status.role, 'status');
  assert.equal(status['aria-live'], 'polite');
  assert.equal(status['aria-atomic'], 'true');
  assert.equal(elements.filter(element => element.attrs.role === 'status').length, 1);
});

test('help covers current ready, retry, remapping and escape contracts without inventing game controls', () => {
  const help = text(html.slice(html.indexOf('<details'), html.indexOf('</details>')));
  assert.match(help, /Enter at the ready screen/);
  assert.match(help, /R starts a fresh three-ball game and clears the score/);
  assert.match(help, /Enter also retries after game-over.*separate press to launch/);
  assert.match(help, /Escape cancels a binding capture.*again to close Controls/);
  assert.match(help, /preserves any existing pause/);
  assert.match(help, /Details → Player tools/);
  assert.match(help, /Escape or Done closes Details and returns focus to the canvas/);
  assert.match(help, /100 points.*five-second save/);
  assert.match(help, /default pointer controls.*lower left or right half/i);
  assert.doesNotMatch(help, /swipe|tilt|nudge|leaderboard|touch.only/i);
  assert.equal(withId('pinball-help-summary').tag, 'summary');
  assert.equal(elements.filter(element => element.tag === 'button').length, 2, 'shared runtime creates Details/Fullscreen/Player tools');
  for (const hook of ['data-controls', 'data-audio']) {
    const matches = elements.filter(element => hook in element.attrs);
    assert.equal(matches.length, 1);
    assert.equal(matches[0].attrs.type, 'button');
    assert.equal(matches[0].attrs['aria-pressed'], 'false');
    assert.ok('disabled' in matches[0].attrs, 'boot owns enabling the controls');
  }
});

test('source routes retain catalogue return, common runtime, and the real game slug/Wasm filename', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const game = catalog.find(entry => entry.id === withId('pinball-table').attrs['data-game']);
  assert.equal(game.id, 'salvage-pinball');
  assert.equal(game.host, 'salvage-pinball');
  assert.equal(game.id.replaceAll('-', '_') + '.wasm', 'salvage_pinball.wasm');
  const external = scripts.filter(([, attrs]) => attributes(attrs).src).map(([, attrs]) => attributes(attrs));
  assert.deepEqual(external, [{type: 'module', src: '../runtime.js'}]);
  assert.ok(statSync(new URL(external[0].src, page)).isFile());
  const links = elements.filter(element => element.tag === 'a');
  assert.deepEqual(links.map(link => link.attrs.href), ['../']);
  assert.ok(statSync(new URL('../index.html', page)).isFile());
  for (const prefix of ['/web/', '/play/', '/project/play/']) {
    const route = new URL(`${prefix}${game.host}/?look=warm&autoplay`, 'https://example.test');
    assert.equal(new URL(links[0].attrs.href, route).pathname, prefix);
    assert.equal(new URL(external[0].src, route).pathname, `${prefix}runtime.js`);
    assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, route).pathname, `${prefix}salvage-pinball/salvage_pinball.wasm`);
  }
  assert.equal(inline.length, 1);
  assert.doesNotMatch(inline[0][2], /WebAssembly|ferravex_|requestAnimationFrame|setInterval|fetch\(/, 'host focus code must not replace ABI, simulation, renderer or lifecycle');
  assert.ok(html.indexOf(inline[0][0]) < html.indexOf('src="../runtime.js"'));
});

test('layout has visible focus, wrapping tools, scoped status and a readable narrow-screen help layout', () => {
  assert.match(css, /#pinball-table:focus-visible\{[^}]*outline:3px solid[^}]*outline-offset:-4px/);
  assert.match(css, /a:focus-visible,button:focus-visible,summary:focus-visible\{[^}]*outline:3px solid/);
  assert.match(css, /forced-colors:active/);
  assert.doesNotMatch(css, /outline:none|clip-path|text-overflow:ellipsis/);
  assert.match(css, /#pinball-toolbar\{[^}]*flex-wrap:wrap[^}]*height:auto/);
  assert.match(css, /#pinball-toolbar>button\{[^}]*min-height:44px/);
  assert.match(css, /#pinball-main\{[^}]*overflow:auto/);
  assert.match(css, /@media\(max-width:540px\)[\s\S]*grid-template-columns:minmax\(0,1fr\)/);
  assert.match(css, /#pinball-status\{/);
  assert.doesNotMatch(css, /\[role\s*=\s*["']?status/, 'do not restyle live regions inserted by Player tools');
  assert.doesNotMatch(html, /user-scalable=no|maximum-scale=/);
});

// This fixture executes the actual inline host script together with bindInput.
// Only event bubbling and focus bookkeeping are doubled, not keyboard mapping.
function fixture() {
  const mutations = [];
  class MutationObserver {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { mutations.push({target, options, callback: this.callback}); }
  }
  class Target {
    constructor(tag) { this.tag = tag; this.listeners = new Map(); this.attributes = new Map(); this.writes = 0; }
    addEventListener(type, listener) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(listener);
    }
    closest(selector) { return selector.split(',').includes(this.tag) ? this : null; }
    focus() { doc.activeElement = this; }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    setAttribute(name, value) { this.attributes.set(name, value); this.writes++; }
  }
  const win = new Target('window'), doc = new Target('document');
  const canvas = new Target('canvas'), summary = new Target('summary');
  const link = new Target('a'), button = new Target('button'), select = new Target('select');
  doc.activeElement = canvas;
  canvas.setAttribute('aria-describedby', withId('pinball-table').attrs['aria-describedby']);
  doc.hasFocus = () => true;
  doc.querySelector = selector => ({'#pinball-table': canvas, '#pinball-help-summary': summary})[selector];
  runInNewContext(inline[0][2], {document: doc, MutationObserver});
  const calls = [];
  let panel = false;
  const wasm = {
    ferravex_key: (...args) => calls.push(['key', ...args]),
    ferravex_blur: () => calls.push(['blur']),
    ferravex_focus: value => calls.push(['focus', value]),
    ferravex_controls_open: () => Number(panel),
    ferravex_input_settings: () => { panel = !panel; },
  };
  const bridge = bindInput(canvas, 'salvage-pinball', () => wasm, () => true, {
    window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  function send(target, type, properties = {}) {
    const event = {target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; }, ...properties};
    for (const current of [target, doc, win]) {
      for (const listener of current.listeners.get(type) ?? []) listener(event);
      if (event.stopped) break;
    }
    return event;
  }
  return {doc, canvas, summary, link, button, select, calls, bridge, send, mutations};
}

test('shared Player tools description coexists with visible host help without a mutation loop', () => {
  const f = fixture();
  assert.equal(f.mutations.length, 1);
  const observer = f.mutations[0];
  assert.equal(observer.target, f.canvas);
  assert.equal(JSON.stringify(observer.options), JSON.stringify({attributes: true, attributeFilter: ['aria-describedby']}));
  const initial = withId('pinball-table').attrs['aria-describedby'];
  for (const shared of ['ferravex-canvas-help', 'ferravex-canvas-help extra-help ferravex-canvas-help', '', null]) {
    if (shared === null) f.canvas.attributes.delete('aria-describedby');
    else f.canvas.setAttribute('aria-describedby', shared);
    observer.callback();
    assert.equal(f.canvas.getAttribute('aria-describedby'), [initial, ...new Set((shared ?? '').split(/\s+/).filter(Boolean))].join(' ').trim());
    const writes = f.canvas.writes;
    observer.callback();
    assert.equal(f.canvas.writes, writes, 'second mutation callback must be a no-op');
    assert.deepEqual(f.calls, []);
  }
});

test('Shift+Tab exits to help before boot and while controls are open, without changing game/menu state', () => {
  for (const open of [false, true]) {
    const f = fixture();
    if (open) f.bridge.openSettings();
    const event = f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true});
    assert.equal(event.defaultPrevented, true);
    assert.equal(f.doc.activeElement, f.summary);
    assert.deepEqual(f.calls, [], 'no synthetic pause, launch, focus reset or other Wasm calls');
    assert.equal(f.bridge.syncPanel(), open);
    assert.equal(f.send(f.summary, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true}).defaultPrevented, false, 'native backward navigation from help is available');
  }
  const focused = [];
  const listeners = new Map();
  const canvas = {addEventListener: (type, listener) => listeners.set(type, listener), getAttribute: () => withId('pinball-table').attrs['aria-describedby']};
  runInNewContext(inline[0][2], {MutationObserver: class {observe() {}}, document: {
    querySelector: selector => selector === '#pinball-table' ? canvas : {focus: () => focused.push('help')},
    addEventListener() {},
  }});
  listeners.get('keydown')({key: 'Tab', shiftKey: true, preventDefault() {}, stopPropagation() {}});
  assert.deepEqual(focused, ['help'], 'exit works even if loading never reaches bindInput');
});

test('gameplay keys, Escape, F2 and plain Tab retain the shared keyboard ABI', () => {
  const f = fixture();
  for (const [code, key, abi] of [['KeyA', 'a', 97], ['ArrowRight', 'ArrowRight', 0xff53], ['Space', ' ', 32], ['Enter', 'Enter', 0xff0d], ['KeyR', 'r', 114], ['Escape', 'Escape', 0xff1b], ['F2', 'F2', 0xffbf], ['Tab', 'Tab', 0xff09]]) {
    assert.equal(f.send(f.canvas, 'keydown', {code, key}).defaultPrevented, true);
    f.send(f.canvas, 'keyup', {code, key});
    assert.deepEqual(f.calls.splice(0), [['key', abi, 1], ['key', abi, 0]], code);
    assert.equal(f.doc.activeElement, f.canvas);
  }
});

test('native page activation/navigation stays out of gameplay and held keys still release after focus leaves', () => {
  const f = fixture();
  for (const target of [f.summary, f.link, f.button, f.select]) {
    for (const [key, code] of [['Enter', 'Enter'], [' ', 'Space'], ['Tab', 'Tab'], ['ArrowDown', 'ArrowDown'], ['Escape', 'Escape']]) {
      target.focus();
      assert.equal(f.send(target, 'keydown', {key, code}).defaultPrevented, false);
      assert.deepEqual(f.calls, [], `${target.tag} ${code} must not press a game action`);
      assert.equal(f.doc.activeElement, target);
    }
  }
  f.canvas.focus();
  f.send(f.canvas, 'keydown', {key: 'a', code: 'KeyA'});
  f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true});
  f.send(f.summary, 'keyup', {key: 'a', code: 'KeyA'});
  assert.deepEqual(f.calls, [['key', 97, 1], ['key', 97, 0]], 'no stuck flipper after leaving the canvas');
});

test('focus exit does not steal browser-modified shortcuts or composition events', () => {
  for (const extra of [{ctrlKey: true}, {altKey: true}, {metaKey: true}, {isComposing: true}, {keyCode: 229}]) {
    const f = fixture();
    const event = f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true, ...extra});
    assert.equal(event.defaultPrevented, false);
    assert.equal(f.doc.activeElement, f.canvas);
    assert.deepEqual(f.calls, []);
  }
});
